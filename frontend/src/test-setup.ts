import "@testing-library/jest-dom/vitest";

// Tests laufen mit deutscher Oberfläche: jsdom meldet sonst „en-US“, und „Automatisch“ würde Englisch wählen.
Object.defineProperty(window.navigator, "language", { value: "de-DE", configurable: true });
Object.defineProperty(window.navigator, "languages", { value: ["de-DE", "de"], configurable: true });
