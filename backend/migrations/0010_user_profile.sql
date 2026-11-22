-- 0010: Benutzerprofil (Uli 16.11.2026). display_name ist der Spitzname (Pflicht);
-- dazu freiwillig Vorname, Nachname, Geburtstag (ISO-Datum, Zweck: Alter für spätere HF-Zonen und Auswertungen)
-- und die Farbe des Benutzersymbols. Ein eigenes Bild folgt später (gemeinsam mit Bildern für Ausrüstung, E1).
ALTER TABLE users ADD COLUMN first_name TEXT;
ALTER TABLE users ADD COLUMN last_name TEXT;
ALTER TABLE users ADD COLUMN birthday TEXT CHECK (birthday IS NULL OR date(birthday) = birthday);
ALTER TABLE users ADD COLUMN avatar_color TEXT;
