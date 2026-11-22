import { useState } from "react";
import { NavLink } from "react-router-dom";
import { Menu, Settings, Upload } from "lucide-react";
import { brand } from "../brand";
import { useProfile } from "../pages/account/useProfile";
import { UserAvatar } from "./UserAvatar";
import { t, type TextKey } from "../i18n";

const main: { to: string; key: TextKey }[] = [
  { to: "/aktivitaeten", key: "nav.activities" },
  { to: "/segmente", key: "nav.segments" },
  { to: "/strecken", key: "nav.routes" },
  { to: "/statistik", key: "nav.statistics" },
  { to: "/form", key: "nav.form" },
];
const side: { to: string; key: TextKey }[] = [
  { to: "/sicherung", key: "nav.backup" },
  { to: "/system", key: "nav.system" },
];

function linkClass({ isActive }: { isActive: boolean }) {
  return `rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
    isActive ? "bg-card-2 text-accent" : "text-muted hover:text-text"
  }`;
}

/** Kopfzeile nach RFC-0006: Logo zum Dashboard, links die Bereiche, rechts Sicherung, System, abgesetzt Import, Zahnrad
 *  (Einstellungen) und ganz rechts das Benutzersymbol zur Kontoseite. Hell/Dunkel steht unter
 *  Einstellungen → Darstellung.
 *  Die volle Zeile braucht gut 850 px; darunter (auch auf dem Tablet) zeigt sie das Menü-Symbol. */
export function Header() {
  const [open, setOpen] = useState(false);
  const profile = useProfile().data?.data;

  return (
    <header className="sticky top-0 z-10 border-b border-line bg-card">
      <div className="relative mx-auto flex max-w-[1400px] items-center gap-2 px-4 py-2">
        <NavLink to="/" className="mr-2 flex shrink-0 items-center gap-2 font-semibold" onClick={() => setOpen(false)}>
          <img src={brand.icon} alt="" className="h-7 w-7 rounded-md" />
          <span>{brand.name}</span>
        </NavLink>
        <button
          type="button"
          className="ml-auto rounded-lg p-2 text-muted hover:text-text lg:hidden"
          aria-label={t("nav.menu")}
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
        >
          <Menu size={20} />
        </button>
        <nav
          className={`${open ? "flex" : "hidden"} absolute left-0 right-0 top-full flex-col gap-1 border-b border-line bg-card p-3 lg:static lg:flex lg:flex-1 lg:flex-row lg:items-center lg:border-0 lg:bg-transparent lg:p-0`}
          onClick={() => setOpen(false)}
        >
          {main.map((l) => (
            <NavLink key={l.to} to={l.to} className={linkClass}>
              {t(l.key)}
            </NavLink>
          ))}
          <div className="lg:ml-auto" />
          {side.map((l) => (
            <NavLink key={l.to} to={l.to} className={linkClass}>
              {t(l.key)}
            </NavLink>
          ))}
          <span aria-hidden="true" className="my-2 h-px bg-muted/40 lg:mx-4 lg:my-0 lg:h-7 lg:w-px" />
          <NavLink
            to="/import"
            className="flex items-center gap-1.5 rounded-lg bg-accent-strong px-3 py-1.5 text-sm font-medium text-white hover:bg-accent"
          >
            <Upload size={16} />
            {t("nav.import")}
          </NavLink>
        </nav>
        <NavLink
          to="/einstellungen"
          className={({ isActive }) => `rounded-lg p-2 hover:text-text ${isActive ? "text-accent" : "text-muted"}`}
          title={t("nav.settings")}
          aria-label={t("nav.settings")}
          onClick={() => setOpen(false)}
        >
          <Settings size={18} />
        </NavLink>
        <NavLink
          to="/konto"
          className={({ isActive }) => `rounded-full p-1 ${isActive ? "ring-2 ring-accent" : "hover:ring-2 hover:ring-line"}`}
          title={profile ? `${t("nav.account")}: ${profile.nickname}` : t("nav.account")}
          aria-label={profile ? `${t("nav.account")}: ${profile.nickname}` : t("nav.account")}
          onClick={() => setOpen(false)}
        >
          <UserAvatar nickname={profile?.nickname} color={profile?.avatar_color} />
        </NavLink>
      </div>
    </header>
  );
}
