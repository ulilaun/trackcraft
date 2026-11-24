// Keine sichtbaren Texte fest im Komponenten-Code (RFC-0006 „Sprache und Formate“): JSX-Text und Attribute wie
// title, aria-label, placeholder und alt laufen über t(). Erlaubt sind Einheiten und Zeichen, die in allen Sprachen gleich
// sind. Einzelne Ausnahmen markiert ein Kommentar `i18n-ignore` in der Zeile davor oder in derselben Zeile.
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import ts from "typescript";
import { describe, expect, it } from "vitest";

const SRC = path.resolve(__dirname, "..");
const ATTRIBUTES = new Set(["title", "aria-label", "placeholder", "alt"]);
/** Einheiten und Kürzel, die nicht übersetzt werden (Einheiten später über format.ts, #42 „Einheiten“). */
const ALLOWED = new Set(["km", "m", "h", "MB", "bpm", "km/h", "/km", "PR", "Z", "h:mm:ss", "https://"]);

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = path.join(dir, f);
    if (statSync(p).isDirectory()) return files(p);
    return p.endsWith(".tsx") && !p.includes(".test.") ? [p] : [];
  });
}

function allowed(text: string): boolean {
  return text
    .split(/[\s·,()–-]+/)
    .filter((w) => /\p{L}/u.test(w))
    .every((w) => ALLOWED.has(w));
}

function findings(file: string, source = readFileSync(file, "utf8")): string[] {
  const lines = source.split("\n");
  const sf = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const out: string[] = [];
  const report = (node: ts.Node, text: string) => {
    const line = sf.getLineAndCharacterOfPosition(node.getStart()).line;
    if (/i18n-ignore/.test(lines[line] ?? "") || /i18n-ignore/.test(lines[line - 1] ?? "")) return;
    out.push(`${path.relative(SRC, file)}:${line + 1} ${JSON.stringify(text)}`);
  };
  const visit = (node: ts.Node) => {
    if (ts.isJsxText(node)) {
      const text = node.text.trim();
      if (/\p{L}/u.test(text) && !allowed(text)) report(node, text);
    } else if (
      ts.isJsxAttribute(node) &&
      ATTRIBUTES.has(node.name.getText()) &&
      node.initializer &&
      ts.isStringLiteral(node.initializer) &&
      /\p{L}/u.test(node.initializer.text) &&
      !allowed(node.initializer.text)
    ) {
      report(node, `${node.name.getText()}="${node.initializer.text}"`);
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return out;
}

describe("Texte im Komponenten-Code", () => {
  it("stehen nicht fest im JSX, sondern kommen aus den Sprachdateien", () => {
    expect(files(SRC).flatMap((f) => findings(f))).toEqual([]);
  });

  it("kein festes Gebietsschema außerhalb von i18n und utils/format.ts", () => {
    const all = (dir: string): string[] =>
      readdirSync(dir).flatMap((f) => {
        const p = path.join(dir, f);
        if (statSync(p).isDirectory()) return all(p);
        return /\.tsx?$/.test(p) && !p.includes(".test.") && !p.endsWith("test-setup.ts") ? [p] : [];
      });
    const hits = all(SRC)
      .filter((f) => !f.includes(`${path.sep}i18n${path.sep}`) && !f.endsWith(path.join("utils", "format.ts")))
      .filter((f) => /["'](de|en)-(DE|GB|US)["']/.test(readFileSync(f, "utf8")))
      .map((f) => path.relative(SRC, f));
    expect(hits).toEqual([]);
  });

  it("findet feste Texte und beachtet i18n-ignore (Gegenprobe)", () => {
    const probe = [
      `export const X = () => <p title="Hallo Welt">Neu laden</p>;`,
      `export const Y = () => <span>{t("nav.plan")} km</span>;`,
      `// i18n-ignore`,
      `export const Z = () => <code>Beispiel</code>;`,
    ].join("\n");
    expect(findings(path.join(SRC, "probe.tsx"), probe)).toEqual(['probe.tsx:1 "title=\\"Hallo Welt\\""', 'probe.tsx:1 "Neu laden"']);
  });
});
