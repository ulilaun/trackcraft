// Animiertes GIF für das README: Segment auf der Karte einer Aktivität markieren, anlegen, Bestenliste ansehen.
// Braucht eine frische Demo-Instanz (z. B. `trackcraft demo start --reset --no-browser`), weil das Segment angelegt wird.
// Aufruf (im Ordner scripts/demo):  npm ci && npm run segment-gif -- [https://localhost:8443]
// Umgebung: PW_CHANNEL=chrome nutzt einen installierten Browser; GIF_FROM/GIF_TO = Anteil der Fahrt (Vorgabe 0.2/0.4),
// GIF_SCALE = Bildgröße (Vorgabe 0.6), GIF_FRAMES_DIR = Ordner für Kontrollbilder.
import { chromium } from "playwright";
import gifenc from "gifenc";
import pngjs from "pngjs";
import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const { GIFEncoder, quantize, applyPalette } = gifenc;
const { PNG } = pngjs;
const BASE = process.argv[2] ?? "https://localhost:8443";
const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../docs/screenshots/segment-erstellen.gif");
// Hin- und Rückweg: der Anstieg liegt im ersten Teil der Fahrt
const FROM = Number(process.env.GIF_FROM ?? 0.2);
const TO = Number(process.env.GIF_TO ?? 0.4);
const NAME = process.env.GIF_NAME ?? "Anstieg nach Lluc";
const VIEWPORT = { width: 1200, height: 760 };

process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
async function api(p) {
  const r = await fetch(BASE + "/api/v1" + p);
  if (!r.ok) throw new Error(`${p}: ${r.status}`);
  return (await r.json()).data;
}

// Eine Fahrt Pollença–Lluc: die mit der Bestzeit am Coll de Femenies
const segments = await api("/segments");
const femenies = segments.find((s) => s.name === "Coll de Femenies");
if (!femenies) throw new Error("Demo-Segment „Coll de Femenies“ fehlt");
if (segments.some((s) => s.name === NAME)) throw new Error(`„${NAME}“ gibt es schon, Demo mit --reset neu starten`);
const rideId = femenies.summary.best.activity_id;

const browser = await chromium.launch(process.env.PW_CHANNEL ? { channel: process.env.PW_CHANNEL } : {});
// deviceScaleFactor < 1 verkleinert die Bilder bei gleichem Layout (GIF-Größe)
const ctx = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: Number(process.env.GIF_SCALE ?? 0.6), colorScheme: "dark", locale: "de-DE", timezoneId: "Europe/Berlin", ignoreHTTPSErrors: true });
// Sichtbarer Mauszeiger, Screenshots zeigen den echten nicht
await ctx.addInitScript(() => {
  addEventListener("DOMContentLoaded", () => {
    const c = document.createElement("div");
    c.style.cssText =
      "position:fixed;z-index:2147483647;left:0;top:0;width:18px;height:18px;margin:-9px 0 0 -9px;border-radius:50%;" +
      "background:rgba(255,255,255,.85);border:2px solid #f97316;pointer-events:none;transition:transform .08s;display:none";
    document.body.appendChild(c);
    addEventListener("pointermove", (e) => {
      c.style.display = "block";
      c.style.left = e.clientX + "px";
      c.style.top = e.clientY + "px";
    }, true);
    addEventListener("pointerdown", () => (c.style.transform = "scale(.7)"), true);
    addEventListener("pointerup", () => (c.style.transform = ""), true);
  });
});
const page = await ctx.newPage();

// Bilder laufend aufnehmen, Pausen werden zu längeren Einzelbildern
const frames = [];
let recording = false;
async function recorder() {
  while (recording) {
    const t = Date.now();
    frames.push({ t, png: await page.screenshot({ type: "png" }) });
    const wait = 150 - (Date.now() - t);
    if (wait > 0) await page.waitForTimeout(wait);
  }
}
const pause = (ms) => page.waitForTimeout(ms);
async function glide(from, to, steps = 18) {
  await page.mouse.move(from.x, from.y);
  for (let i = 1; i <= steps; i++) {
    await page.mouse.move(from.x + ((to.x - from.x) * i) / steps, from.y + ((to.y - from.y) * i) / steps);
    await pause(25);
  }
}
const center = async (loc) => {
  const b = await loc.boundingBox();
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
};

await page.goto(`${BASE}/aktivitaeten/${rideId}`, { waitUntil: "networkidle" });
await pause(1500);
recording = true;
const rec = recorder();
await pause(1200);

// „Segment erstellen“ in den Top-Ergebnissen
const createBtn = page.getByRole("button", { name: "Segment erstellen" });
await glide({ x: VIEWPORT.width / 2, y: VIEWPORT.height - 80 }, await center(createBtn));
await pause(300);
await createBtn.click();
const dialog = page.getByRole("dialog");
await dialog.waitFor();
await pause(1800); // Karte und Profil laden

// Start- und Zielregler auf der Profilschiene ziehen
const startSlider = dialog.getByRole("slider", { name: "Start" });
const rail = await startSlider.locator("..").boundingBox();
const startHandle = await center(startSlider);
const endHandle = await center(dialog.getByRole("slider", { name: "Ziel" }));
const x0 = rail.x;
const w = rail.width;
async function drag(from, toX) {
  await glide({ x: from.x - 60, y: from.y - 40 }, from, 8);
  await page.mouse.down();
  await glide(from, { x: toX, y: from.y }, 26);
  await page.mouse.up();
  await pause(500);
}
// Die Regler dürfen sich nicht kreuzen: liegt das Ziel vor dem bisherigen Start, zuerst den Start ziehen
if (x0 + w * TO < startHandle.x) {
  await drag(startHandle, x0 + w * FROM);
  await drag(endHandle, x0 + w * TO);
} else {
  await drag(endHandle, x0 + w * TO);
  await drag(startHandle, x0 + w * FROM);
}
await pause(900);

// Name eintragen und anlegen
const name = dialog.getByPlaceholder("z. B. Anstieg Wald");
await glide(endHandle, await center(name), 14);
await name.click();
await name.pressSequentially(NAME, { delay: 60 });
await pause(600);
const submit = dialog.getByRole("button", { name: "Segment erstellen" });
if (await submit.isDisabled()) {
  const hint = await dialog.getByTestId("create-message").innerText();
  throw new Error(`„Segment erstellen“ gesperrt: ${hint} (GIF_FROM/GIF_TO anpassen)`);
}
const submitAt = await center(submit);
await glide(await center(name), submitAt, 14);
await submit.click();

// Auswertung über alle Aktivitäten abwarten
await page.getByText(/Versuche/).first().waitFor({ timeout: 120_000 });
await pause(2000);
const close = page.getByRole("dialog").getByRole("button", { name: "Schließen" }).last();
await glide(submitAt, await center(close), 10);
await close.click();
await pause(1200);

// Zur Bestenliste des neuen Segments
const link = page.getByRole("link", { name: new RegExp(NAME) }).first();
await link.scrollIntoViewIfNeeded();
await pause(600);
await glide({ x: VIEWPORT.width / 2, y: VIEWPORT.height / 2 }, await center(link), 12);
await link.click();
await page.waitForLoadState("networkidle");
await pause(2500);
// Bestenliste unter Karte und Profil zeigen
const board = page.getByText("Bestenliste", { exact: false }).first();
const target = (await board.count()) > 0 ? (await board.boundingBox())?.y ?? 700 : 700;
for (let y = 0; y < target - 120; y += 60) {
  await page.mouse.wheel(0, 60);
  await pause(40);
}
await pause(3500);
recording = false;
await rec;
await browser.close();

// GIF: gleiche Bilder zusammenfassen, gemeinsame Palette aus Stichproben, Verzögerung aus den Zeitstempeln
for (let i = frames.length - 1; i > 0; i--) if (frames[i].png.equals(frames[i - 1].png)) frames.splice(i, 1);
if (process.env.GIF_FRAMES_DIR) {
  // Zur Kontrolle jedes zehnte Bild als PNG
  for (let i = 0; i < frames.length; i += 10) await writeFile(path.join(process.env.GIF_FRAMES_DIR, `f${String(i).padStart(3, "0")}.png`), frames[i].png);
}
const decoded = frames.map((f) => PNG.sync.read(f.png));
const { width, height } = decoded[0];
const picks = decoded.filter((_, i) => i % 8 === 0);
const sample = new Uint8Array(picks.length * picks[0].data.length);
picks.forEach((d, i) => sample.set(d.data, i * d.data.length));
// 255 Farben plus ein durchsichtiger Eintrag: Pixel wie im Vorbild bleiben durchsichtig, das spart viel Platz
const palette = quantize(sample, 255);
while (palette.length < 255) palette.push([0, 0, 0]);
const CLEAR = 255;
palette.push([255, 0, 255]);
const gif = GIFEncoder();
decoded.forEach((d, i) => {
  const next = frames[i + 1]?.t ?? frames[i].t + 2500;
  const delay = Math.min(3000, Math.max(80, next - frames[i].t));
  const index = applyPalette(d.data, palette.slice(0, 255));
  if (i > 0) {
    const prev = decoded[i - 1].data;
    for (let p = 0; p < index.length; p++) {
      const o = p * 4;
      if (d.data[o] === prev[o] && d.data[o + 1] === prev[o + 1] && d.data[o + 2] === prev[o + 2]) index[p] = CLEAR;
    }
  }
  gif.writeFrame(index, width, height, {
    palette: i === 0 ? palette : undefined,
    delay,
    repeat: 0,
    transparent: i > 0,
    transparentIndex: CLEAR,
    dispose: 1, // vorheriges Bild stehen lassen, durchsichtige Pixel zeigen es

  });
});
gif.finish();
await writeFile(OUT, gif.bytes());
console.log(OUT, frames.length, "Bilder", Math.round(gif.bytes().length / 1024), "KB");
