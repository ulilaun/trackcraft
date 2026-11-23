// Screenshots der Oberfläche aus einer mit seed_demo.py befüllten Test-Instanz, hell und dunkel.
// Aufruf (im Ordner scripts/demo):  npm ci && npx playwright install chromium && npm run screenshots -- [https://localhost:8443]
import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const BASE = process.argv[2] ?? "https://localhost:8443";
const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../docs/screenshots");

async function api(p) {
  const r = await fetch(BASE + "/api/v1" + p);
  if (!r.ok) throw new Error(`${p}: ${r.status}`);
  return (await r.json()).data;
}

// Selbst signiertes Zertifikat der Test-Instanz
process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";

const activities = await api("/activities?limit=100");
const segments = await api("/segments");
const calobra = segments.find((s) => s.name === "Sa Calobra");
const ride = activities.find((a) => a.id === calobra.summary.best.activity_id);

const desktop = { width: 1440, height: 960 };
const tablet = { width: 1024, height: 1366 };
const shots = [
  { name: "dashboard", url: "/", viewport: desktop },
  { name: "aktivitaeten", url: "/aktivitaeten", viewport: desktop },
  { name: "aktivitaet", url: `/aktivitaeten/${ride.id}`, viewport: desktop },
  { name: "segment", url: `/segmente/${calobra.id}`, viewport: desktop },
  { name: "statistik", url: "/statistik", viewport: desktop },
  { name: "form", url: "/form", viewport: desktop },
  { name: "plan", url: "/plan", viewport: desktop },
  { name: "tablet-aktivitaet", url: `/aktivitaeten/${ride.id}`, viewport: tablet, scale: 1 },
];

await mkdir(OUT, { recursive: true });
// PW_CHANNEL=chrome (oder msedge) nutzt einen installierten Browser statt des Playwright-Chromiums
const browser = await chromium.launch(process.env.PW_CHANNEL ? { channel: process.env.PW_CHANNEL } : {});
for (const scheme of ["light", "dark"]) {
  for (const s of shots) {
    const ctx = await browser.newContext({
      viewport: s.viewport,
      deviceScaleFactor: s.scale ?? 1,
      colorScheme: scheme,
      locale: "de-DE",
      timezoneId: "Europe/Berlin",
      ignoreHTTPSErrors: true,
    });
    const page = await ctx.newPage();
    await page.goto(BASE + s.url, { waitUntil: "networkidle" });
    await page.waitForTimeout(1500); // Kacheln, Diagramm-Animationen
    const file = path.join(OUT, `${s.name}-${scheme === "light" ? "hell" : "dunkel"}.png`);
    await page.screenshot({ path: file });
    console.log(file);
    await ctx.close();
  }
}

// Handybreite: keine Seite darf breiter als der Bildschirm sein, breite Inhalte scrollen nur in ihrem Kasten.
const phone = await browser.newContext({ viewport: { width: 360, height: 780 }, locale: "de-DE", timezoneId: "Europe/Berlin", ignoreHTTPSErrors: true });
const page = await phone.newPage();
const pages = ["/", "/aktivitaeten", `/aktivitaeten/${ride.id}`, "/segmente", "/strecken", `/segmente/${calobra.id}`, "/statistik", "/form", "/plan", "/import", "/system", "/einstellungen", "/konto"];
const tooWide = [];
for (const url of pages) {
  await page.goto(BASE + url, { waitUntil: "networkidle" });
  await page.waitForTimeout(500);
  const [scroll, client] = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]);
  if (scroll > client) tooWide.push(`${url}: ${scroll} px statt ${client} px`);
}
await phone.close();
await browser.close();
if (tooWide.length) {
  console.error(`Auf Handybreite zu breit:\n  ${tooWide.join("\n  ")}`);
  process.exitCode = 1;
} else {
  console.log(`Handybreite 360 px: alle ${pages.length} Seiten passen.`);
}
