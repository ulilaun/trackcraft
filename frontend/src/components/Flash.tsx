import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";

type Show = (message: string) => void;

const FlashContext = createContext<Show>(() => {});

/** Kurze Meldung unten auf der Seite, die einen Seitenwechsel überdauert (z. B. „Aktivität gelöscht“). */
export function FlashProvider({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState<{ text: string; n: number } | null>(null);
  const show = useCallback<Show>((text) => setMessage((m) => ({ text, n: (m?.n ?? 0) + 1 })), []);

  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(() => setMessage(null), 4000);
    return () => clearTimeout(timer);
  }, [message]);

  return (
    <FlashContext.Provider value={show}>
      {children}
      {message && (
        <div role="status" className="fixed bottom-4 left-1/2 z-[2100] -translate-x-1/2 rounded-lg border border-line bg-card px-4 py-2 text-sm shadow-lg">
          {message.text}
        </div>
      )}
    </FlashContext.Provider>
  );
}

export function useFlash(): Show {
  return useContext(FlashContext);
}
