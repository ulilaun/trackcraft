import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Pencil, Plus, RotateCcw, Trash2, TriangleAlert, X } from "lucide-react";
import {
  ApiError,
  apiDelete,
  apiGet,
  apiPatch,
  apiPost,
  apiPut,
  type SportClass,
  type SportMapping,
  type SportMappingListResponse,
  type SubSport,
  type SubSportListResponse,
} from "../../api/client";
import { Card } from "../../components/Card";
import { Problem, problemText } from "../../components/Problem";
import { IconSelect } from "../../components/IconSelect";
import { SportIcon, SubSportIcon } from "../../components/SportIcon";
import type { SportGlyphId } from "../../components/glyphs";
import { t, type TextKey } from "../../i18n";
import { defaultSubSportGlyph, SPORT_CLASSES, subSportEntryLabel, subSportGlyph } from "../../utils/sports";
import { GlyphPicker } from "./GlyphPicker";
import { CATALOG_CLASSES, classChange, inheritedFrom, needsAttention, normalizeSource, sortMappings, subsOf } from "./sportsModel";

const input = "rounded-lg border border-line bg-card-2 px-2 py-1 text-sm text-text disabled:opacity-60";
const iconButton = "rounded-md p-1 text-muted hover:bg-card-2 hover:text-text disabled:opacity-40 disabled:hover:bg-transparent";

function useSportData() {
  const mappings = useQuery({ queryKey: ["sport-mappings"], queryFn: () => apiGet<SportMappingListResponse>("/sport-mappings") });
  const subs = useQuery({ queryKey: ["sub-sports"], queryFn: () => apiGet<SubSportListResponse>("/sub-sports") });
  return { mappings, subs };
}

/** Nach jeder Änderung: Tabelle, Katalog und den Katalog in `GET /config` (Dialoge, Filter, Anzeige) neu laden. */
function useRefresh() {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: ["sport-mappings"] });
    void queryClient.invalidateQueries({ queryKey: ["sub-sports"] });
    void queryClient.invalidateQueries({ queryKey: ["config"] });
  };
}

const classLabel = (c: SportClass) => t(`sport.${c}` as TextKey);
const classOption = (c: SportClass) => ({ value: c, label: classLabel(c), icon: <SportIcon sport={c} decorative /> });

// ---------------------------------------------------------------------------------------------------------
// Zuordnung

function MappingRow({ m, subs, busy, onPut, onDelete }: {
  m: SportMapping;
  subs: SubSport[];
  busy: boolean;
  onPut: (body: { sport_class: SportClass; sub_sport: string | null }) => void;
  onDelete: () => void;
}) {
  const attention = needsAttention(m);
  const inherited = inheritedFrom(m);
  const options = m.sport_class ? subsOf(subs, m.sport_class) : [];
  return (
    <tr className={`border-b border-line last:border-0 ${attention ? "bg-warn/10" : ""}`}>
      <td className="px-3 py-2">
        <span className="flex items-center gap-2 font-mono text-xs">
          {attention && <TriangleAlert size={14} className="shrink-0 text-warn" aria-label={t("sports.attention")} />}
          {m.source_sport}
        </span>
      </td>
      <td className="num px-3 py-2 text-right">{m.activity_count}</td>
      <td className="px-3 py-2">
        <IconSelect
          className="min-w-[170px]"
          label={t("sports.col.class")}
          value={m.sport_class ?? ""}
          disabled={busy}
          onChange={(v) => (v ? onPut(classChange(m, v as SportClass, subs)) : onDelete())}
          options={[
            {
              value: "",
              label: inherited ? t("sports.inherits", { sport: inherited, class: classLabel(m.effective_sport_class) }) : t("sports.unmapped"),
              icon: <SportIcon sport={m.effective_sport_class} decorative />,
            },
            ...SPORT_CLASSES.map(classOption),
          ]}
        />
      </td>
      <td className="px-3 py-2">
        <IconSelect
          className="min-w-[150px]"
          label={t("sports.col.subSport")}
          value={m.sub_sport ?? ""}
          disabled={busy || !m.sport_class || options.length === 0}
          onChange={(v) => m.sport_class && onPut({ sport_class: m.sport_class, sub_sport: v || null })}
          options={[
            { value: "", label: t("sports.noSubSport") },
            ...options.map((o) => ({ value: o.key, label: subSportEntryLabel(o), icon: <SubSportIcon sport={o.sport_class} entry={o} decorative /> })),
          ]}
        />
      </td>
    </tr>
  );
}

function MappingCard({ mappings, subs }: { mappings: SportMapping[]; subs: SubSport[] }) {
  const refresh = useRefresh();
  const [source, setSource] = useState("");
  const [newClass, setNewClass] = useState<SportClass>("cycling");
  const put = useMutation({
    mutationFn: ({ src, body }: { src: string; body: { sport_class: SportClass; sub_sport: string | null } }) =>
      apiPut(`/sport-mappings/${encodeURIComponent(src)}`, body),
    onSuccess: refresh,
  });
  const del = useMutation({ mutationFn: (src: string) => apiDelete(`/sport-mappings/${encodeURIComponent(src)}`), onSuccess: refresh });
  const busy = put.isPending || del.isPending;
  const normalized = normalizeSource(source);
  const exists = mappings.some((m) => m.source_sport === normalized && m.mapped);
  const attention = mappings.filter(needsAttention).length;

  return (
    <Card title={t("sports.mappings")} className="min-w-0">
      <p className="mb-3 text-sm text-muted">{t("sports.mappingsHint")}</p>
      {attention > 0 && <p className="mb-3 text-sm text-warn">{t("sports.attentionCount", { count: attention })}</p>}
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-xs text-muted">
            <tr className="border-b border-line">
              <th className="px-3 py-2 text-left font-medium">{t("sports.col.source")}</th>
              <th className="px-3 py-2 text-right font-medium">{t("sports.col.activities")}</th>
              <th className="px-3 py-2 text-left font-medium">{t("sports.col.class")}</th>
              <th className="px-3 py-2 text-left font-medium">{t("sports.col.subSport")}</th>
            </tr>
          </thead>
          <tbody>
            {sortMappings(mappings).map((m) => (
              <MappingRow
                key={m.source_sport}
                m={m}
                subs={subs}
                busy={busy}
                onPut={(body) => put.mutate({ src: m.source_sport, body })}
                onDelete={() => del.mutate(m.source_sport)}
              />
            ))}
          </tbody>
        </table>
      </div>
      <form
        className="mt-3 flex flex-wrap items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (!normalized || exists || busy) return;
          put.mutate({ src: normalized, body: { sport_class: newClass, sub_sport: null } }, { onSuccess: () => setSource("") });
        }}
      >
        <input
          className={`${input} w-56 font-mono`}
          value={source}
          placeholder={t("sports.newSource")}
          aria-label={t("sports.newSource")}
          maxLength={64}
          disabled={busy}
          onChange={(e) => setSource(e.target.value)}
        />
        <IconSelect
          className="w-44"
          label={t("sports.newClass")}
          value={newClass}
          disabled={busy}
          onChange={(v) => setNewClass(v as SportClass)}
          options={SPORT_CLASSES.map(classOption)}
        />
        <button type="submit" className={`${input} flex items-center gap-1 hover:border-accent`} disabled={!normalized || exists || busy}>
          <Plus size={14} />
          {t("sports.add")}
        </button>
        {source && normalized !== source.trim() && <span className="text-xs text-muted">{t("sports.savedAs", { key: normalized })}</span>}
        {exists && <span className="text-xs text-danger">{t("sports.exists")}</span>}
      </form>
      {(put.error ?? del.error) && <Problem error={put.error ?? del.error} />}
    </Card>
  );
}

// ---------------------------------------------------------------------------------------------------------
// Untertyp-Katalog

function usageText(s: SubSport): string {
  return t("sports.usage", { activities: s.activity_count, mappings: s.mapping_count });
}

function SubSportItem({ s, onError }: { s: SubSport; onError: (e: unknown) => void }) {
  const refresh = useRefresh();
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const rename = useMutation({
    mutationFn: (n: string | null) => apiPatch(`/sub-sports/${s.sport_class}/${encodeURIComponent(s.key)}`, { name: n }),
    onSuccess: () => {
      setEditing(false);
      refresh();
    },
    onError,
  });
  const del = useMutation({ mutationFn: () => apiDelete(`/sub-sports/${s.sport_class}/${encodeURIComponent(s.key)}`), onSuccess: refresh, onError });
  const icon = useMutation({
    mutationFn: (i: SportGlyphId | null) => apiPatch(`/sub-sports/${s.sport_class}/${encodeURIComponent(s.key)}`, { icon: i }),
    onSuccess: refresh,
    onError,
  });
  const used = s.activity_count > 0 || s.mapping_count > 0;
  const busy = rename.isPending || del.isPending || icon.isPending;

  if (editing) {
    return (
      <li className="flex items-center gap-1">
        <form
          className="flex items-center gap-1"
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) rename.mutate(name.trim());
          }}
        >
          <input
            className={`${input} w-44`}
            value={name}
            maxLength={40}
            autoFocus
            aria-label={t("sports.renameLabel")}
            disabled={busy}
            onChange={(e) => setName(e.target.value)}
          />
          <button type="submit" className={iconButton} disabled={busy || !name.trim()} aria-label={t("sports.save")} title={t("sports.save")}>
            <Check size={16} />
          </button>
        </form>
        <button type="button" className={iconButton} disabled={busy} onClick={() => setEditing(false)} aria-label={t("sports.cancel")} title={t("sports.cancel")}>
          <X size={16} />
        </button>
      </li>
    );
  }

  const label = subSportEntryLabel(s);
  return (
    <li className="flex items-center gap-2 rounded-lg border border-line bg-card-2 py-1 pr-2.5 pl-1">
      {/* Symbol wählbar */}
      <GlyphPicker
        sport={s.sport_class}
        name={label}
        current={subSportGlyph(s.sport_class, s)}
        chosen={s.icon ?? null}
        fallback={defaultSubSportGlyph(s.sport_class, s.key)}
        disabled={busy}
        onPick={(i) => icon.mutate(i)}
      />
      <span className="font-medium">{label}</span>
      <span className="text-xs text-muted">{usageText(s)}</span>
      <span className="ml-auto flex items-center">
        <button
          type="button"
          className={iconButton}
          disabled={busy}
          onClick={() => {
            setName(label);
            setEditing(true);
          }}
          aria-label={t("sports.rename", { name: label })}
          title={t("sports.rename", { name: label })}
        >
          <Pencil size={14} />
        </button>
        {s.builtin && s.name && (
          <button
            type="button"
            className={iconButton}
            disabled={busy}
            onClick={() => rename.mutate(null)}
            aria-label={t("sports.reset", { name: label })}
            title={t("sports.reset", { name: label })}
          >
            <RotateCcw size={14} />
          </button>
        )}
        <button
          type="button"
          className={iconButton}
          disabled={busy || used}
          onClick={() => del.mutate()}
          aria-label={t("sports.delete", { name: label })}
          title={used ? t("sports.deleteBlocked") : t("sports.delete", { name: label })}
        >
          <Trash2 size={14} />
        </button>
      </span>
    </li>
  );
}

function ClassCatalog({ sportClass, subs }: { sportClass: SportClass; subs: SubSport[] }) {
  const refresh = useRefresh();
  const [name, setName] = useState("");
  const [error, setError] = useState<unknown>(null);
  const create = useMutation({
    mutationFn: () => apiPost("/sub-sports", { sport_class: sportClass, name: name.trim() }),
    onSuccess: () => {
      setName("");
      setError(null);
      refresh();
    },
    onError: setError,
  });
  return (
    <div className="grid gap-2 border-b border-line pb-3 last:border-0 last:pb-0">
      <h3 className="flex items-center gap-2 text-sm font-semibold">
        <SportIcon sport={sportClass} />
        {classLabel(sportClass)}
      </h3>
      <ul className="grid gap-1.5 sm:grid-cols-2 xl:grid-cols-3">
        {subs.map((s) => (
          <SubSportItem key={s.key} s={s} onError={setError} />
        ))}
      </ul>
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim() && !create.isPending) create.mutate();
        }}
      >
        <input
          className={`${input} w-44`}
          value={name}
          maxLength={40}
          placeholder={t("sports.newSubSport")}
          aria-label={t("sports.newSubSportFor", { class: classLabel(sportClass) })}
          disabled={create.isPending}
          onChange={(e) => setName(e.target.value)}
        />
        <button type="submit" className={`${input} flex items-center gap-1 hover:border-accent`} disabled={!name.trim() || create.isPending}>
          <Plus size={14} />
          {t("sports.add")}
        </button>
      </form>
      {error !== null && <p className="text-sm text-danger">{inUseText(error) ?? problemText(error)}</p>}
    </div>
  );
}

/** `sub_sport_in_use` mit Anzahlen aus `details` (Uli 14.11.2026: Löschen gesperrt mit Hinweis). */
function inUseText(e: unknown): string | null {
  if (!(e instanceof ApiError) || e.code !== "sub_sport_in_use") return null;
  const d = e.details as { activity_count?: number; mapping_count?: number };
  return t("sports.inUse", { activities: d.activity_count ?? 0, mappings: d.mapping_count ?? 0 });
}

function CatalogCard({ subs }: { subs: SubSport[] }) {
  return (
    <Card title={t("sports.catalog")} className="min-w-0">
      <p className="mb-3 text-sm text-muted">{t("sports.catalogHint")}</p>
      <div className="grid gap-3">
        {CATALOG_CLASSES.map((c) => (
          <ClassCatalog key={c} sportClass={c} subs={subsOf(subs, c)} />
        ))}
      </div>
    </Card>
  );
}

/** Abschnitt „Sportarten“ der Einstellungen (RFC-0006): Zuordnung der Dateitypen und Untertyp-Katalog. */
export function SportsSettings() {
  const { mappings, subs } = useSportData();
  if (mappings.error || subs.error) return <Problem error={mappings.error ?? subs.error} />;
  if (!mappings.data || !subs.data) return <p className="text-muted">{t("state.loading")}</p>;
  return (
    <div className="grid min-w-0 gap-4">
      <MappingCard mappings={mappings.data.data} subs={subs.data.data} />
      <CatalogCard subs={subs.data.data} />
    </div>
  );
}
