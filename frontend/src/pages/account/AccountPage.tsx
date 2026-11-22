import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check } from "lucide-react";
import { apiPatch, type Profile, type ProfileResponse } from "../../api/client";
import { Card } from "../../components/Card";
import { primaryButton } from "../../components/Dialog";
import { useFlash } from "../../components/Flash";
import { Problem } from "../../components/Problem";
import { UserAvatar } from "../../components/UserAvatar";
import { useConfig } from "../../hooks/useConfig";
import { t, type TextKey } from "../../i18n";
import { localDate } from "../../utils/format";
import { HeartRateZones } from "./HeartRateZones";
import { AVATAR_CLASS, AVATAR_COLORS, age, formOf, formProblem, inputOf, type ProfileForm } from "./profileModel";
import { useProfile } from "./useProfile";

const input = "w-full rounded-lg border border-line bg-card-2 px-2 py-1.5 text-sm text-text disabled:opacity-60";

/** Kontoseite hinter dem Benutzersymbol oben rechts. */
export function AccountPage() {
  const profile = useProfile();
  const today = localDate(new Date(), useConfig().data?.data.timezone);
  return (
    <div className="grid min-w-0 max-w-3xl gap-4">
      <h1 className="text-2xl font-bold">{t("account.title")}</h1>
      {profile.error ? (
        <Problem error={profile.error} />
      ) : profile.data ? (
        // Neu aufbauen, wenn sich das gespeicherte Profil ändert
        <ProfileEditor key={profile.data.data.updated_at} profile={profile.data.data} />
      ) : (
        <p className="text-muted">{t("account.loading")}</p>
      )}
      <HeartRateZones today={today} />
    </div>
  );
}

function ProfileEditor({ profile }: { profile: Profile }) {
  const [form, setForm] = useState<ProfileForm>(() => formOf(profile));
  const timeZone = useConfig().data?.data.timezone;
  const today = localDate(new Date(), timeZone);
  const queryClient = useQueryClient();
  const flash = useFlash();
  const body = inputOf(form, profile);
  const problem = formProblem(form, today);
  const save = useMutation({
    mutationFn: () => apiPatch<ProfileResponse>("/profile", body),
    onSuccess: (r) => {
      queryClient.setQueryData(["profile"], r);
      void queryClient.invalidateQueries({ queryKey: ["config"] });
      flash(t("account.saved"));
    },
  });
  const busy = save.isPending;
  const set = (p: Partial<ProfileForm>) => {
    setForm((f) => ({ ...f, ...p }));
    save.reset();
  };
  const years = age(form.birthday || null, today);
  const fullName = [form.first_name.trim(), form.last_name.trim()].filter(Boolean).join(" ");

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (body && !problem && !busy) save.mutate();
      }}
    >
      <Card>
        <div className="flex items-center gap-4">
          <UserAvatar nickname={form.nickname} color={form.avatar_color} size={64} />
          <div className="min-w-0">
            <div className="truncate text-lg font-semibold" data-testid="account-nickname">
              {form.nickname.trim() || "–"}
            </div>
            {(fullName || years !== null) && (
              <div className="truncate text-sm text-muted">
                {[fullName, years !== null ? t("account.age", { years }) : null].filter(Boolean).join(" · ")}
              </div>
            )}
          </div>
        </div>
        <fieldset className="mt-4">
          <legend className="mb-2 text-xs font-medium text-muted">{t("account.color")}</legend>
          <div role="radiogroup" aria-label={t("account.color")} className="flex flex-wrap gap-2">
            {AVATAR_COLORS.map((c) => {
              const on = (form.avatar_color ?? "blue") === c;
              return (
                <button
                  key={c}
                  type="button"
                  role="radio"
                  aria-checked={on}
                  aria-label={t(`account.color.${c}` as TextKey)}
                  title={t(`account.color.${c}` as TextKey)}
                  disabled={busy}
                  onClick={() => set({ avatar_color: c })}
                  className={`grid h-8 w-8 place-items-center rounded-full text-white ${AVATAR_CLASS[c]} ${on ? "ring-2 ring-text ring-offset-2 ring-offset-card" : ""}`}
                >
                  {on && <Check size={16} />}
                </button>
              );
            })}
          </div>
          <p className="mt-2 text-xs text-muted">{t("account.imageLater")}</p>
        </fieldset>
      </Card>

      <Card>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="grid gap-1 sm:col-span-2">
            <span className="text-xs font-medium text-muted">{t("account.nickname")}</span>
            <input className={input} value={form.nickname} maxLength={40} required disabled={busy} onChange={(e) => set({ nickname: e.target.value })} />
            <span className="text-xs text-muted">{t("account.nicknameHint")}</span>
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("account.firstName")}</span>
            <input className={input} value={form.first_name} maxLength={60} disabled={busy} onChange={(e) => set({ first_name: e.target.value })} />
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("account.lastName")}</span>
            <input className={input} value={form.last_name} maxLength={60} disabled={busy} onChange={(e) => set({ last_name: e.target.value })} />
          </label>
          <label className="grid gap-1">
            <span className="text-xs font-medium text-muted">{t("account.birthday")}</span>
            <input type="date" className={input} value={form.birthday} min="1900-01-01" max={today} disabled={busy} onChange={(e) => set({ birthday: e.target.value })} />
          </label>
          <p className="self-end text-xs text-muted sm:pb-2">{t("account.birthdayHint")}</p>
        </div>
        {problem ? (
          <p className="mt-3 text-sm text-danger" role="alert">
            {t(`account.problem.${problem}` as TextKey)}
          </p>
        ) : (
          save.error && (
            <div className="mt-3">
              <Problem error={save.error} />
            </div>
          )
        )}
        <div className="mt-4 flex justify-end">
          <button type="submit" className={primaryButton} disabled={!body || !!problem || busy}>
            {busy ? t("edit.saving") : t("edit.save")}
          </button>
        </div>
      </Card>
    </form>
  );
}
