import type { AvatarColor } from "../api/client";
import { AVATAR_CLASS, avatarText } from "../pages/account/profileModel";

/**
 * Benutzersymbol: Kreis in der gewählten Farbe mit dem Spitznamen (bis 3 Zeichen) bzw. dessen erstem Zeichen.
 * Ein eigenes Bild folgt später. Ohne Spitzname (Profil lädt noch) ein leerer Kreis.
 */
export function UserAvatar({ nickname, color, size = 28 }: { nickname?: string; color?: AvatarColor | null; size?: 28 | 64 }) {
  const text = nickname ? avatarText(nickname) : "";
  // Ohne Wahl Blau, wie in der Farbauswahl der Kontoseite vorgewählt
  const bg = AVATAR_CLASS[color ?? "blue"];
  const dim = size === 64 ? "h-16 w-16" : "h-7 w-7";
  const font = size === 64 ? (text.length > 2 ? "text-lg" : "text-2xl") : text.length > 2 ? "text-[10px]" : text.length > 1 ? "text-xs" : "text-sm";
  return (
    <span aria-hidden="true" className={`grid shrink-0 place-items-center rounded-full font-semibold leading-none text-white ${bg} ${dim} ${font}`}>
      {text}
    </span>
  );
}
