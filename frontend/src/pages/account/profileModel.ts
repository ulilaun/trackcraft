// Reine Logik der Kontoseite.
import type { AvatarColor, Profile, ProfileInput } from "../../api/client";
import { upperFirst } from "../../utils/format";

export const AVATAR_COLORS: AvatarColor[] = ["blue", "teal", "green", "amber", "orange", "red", "pink", "violet"];

/** Feste Klassen je Farbe, damit Tailwind sie findet (keine Inline-Styles wegen CSP). */
export const AVATAR_CLASS: Record<AvatarColor, string> = {
  blue: "bg-[#2563eb]",
  teal: "bg-[#0d9488]",
  green: "bg-[#16a34a]",
  amber: "bg-[#d97706]",
  orange: "bg-[#ea580c]",
  red: "bg-[#dc2626]",
  pink: "bg-[#db2777]",
  violet: "bg-[#7c3aed]",
};

/** Text im Benutzersymbol (Uli 16.11.2026): Spitzname bis 3 Zeichen ganz, sonst nur das erste Zeichen groß. */
export function avatarText(nickname: string): string {
  const chars = Array.from(nickname.trim());
  if (chars.length === 0) return "?";
  if (chars.length <= 3) return chars.join("");
  return upperFirst(chars[0]!);
}

/** Alter in vollen Jahren an `today` (beide YYYY-MM-DD); null ohne Geburtstag. */
export function age(birthday: string | null, today: string): number | null {
  if (!birthday) return null;
  const [by, bm, bd] = birthday.split("-").map(Number);
  const [ty, tm, td] = today.split("-").map(Number);
  if (!by || !bm || !bd || !ty || !tm || !td) return null;
  return ty - by - (tm < bm || (tm === bm && td < bd) ? 1 : 0);
}

export interface ProfileForm {
  nickname: string;
  first_name: string;
  last_name: string;
  birthday: string;
  avatar_color: AvatarColor | null;
}

export function formOf(p: Profile): ProfileForm {
  return {
    nickname: p.nickname,
    first_name: p.first_name ?? "",
    last_name: p.last_name ?? "",
    birthday: p.birthday ?? "",
    avatar_color: p.avatar_color,
  };
}

/** Nur geänderte Felder; leere freiwillige Felder werden null. `null`: nichts geändert. */
export function inputOf(form: ProfileForm, p: Profile): ProfileInput | null {
  const out: ProfileInput = {};
  const opt = (v: string) => (v.trim() === "" ? null : v.trim());
  if (form.nickname.trim() !== p.nickname) out.nickname = form.nickname.trim();
  if (opt(form.first_name) !== p.first_name) out.first_name = opt(form.first_name);
  if (opt(form.last_name) !== p.last_name) out.last_name = opt(form.last_name);
  if (opt(form.birthday) !== p.birthday) out.birthday = opt(form.birthday);
  if (form.avatar_color !== p.avatar_color) out.avatar_color = form.avatar_color;
  return Object.keys(out).length ? out : null;
}

export type FormProblem = "nicknameRequired" | "birthdayFuture" | null;

export function formProblem(form: ProfileForm, today: string): FormProblem {
  if (form.nickname.trim() === "") return "nicknameRequired";
  if (form.birthday && form.birthday > today) return "birthdayFuture";
  return null;
}
