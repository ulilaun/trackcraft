import { fireEvent, render, screen } from "@testing-library/react";
import { Dialog } from "./Dialog";

function setup() {
  const onClose = vi.fn();
  const view = render(
    <Dialog title="Test" onClose={onClose} footer={<button type="button">OK</button>}>
      <p>Inhalt</p>
    </Dialog>,
  );
  const backdrop = screen.getByRole("dialog").parentElement!;
  return { onClose, backdrop, view };
}

describe("Dialog", () => {
  it("sperrt die Seite dahinter und gibt sie beim Schließen frei", () => {
    document.body.style.overflow = "";
    const { view } = setup();
    expect(document.body.style.overflow).toBe("hidden");
    view.unmount();
    expect(document.body.style.overflow).toBe("");
  });

  it("Antippen des Hintergrunds schließt, ein Wisch nicht", () => {
    const { onClose, backdrop } = setup();
    fireEvent.pointerDown(backdrop, { clientX: 5, clientY: 5 });
    fireEvent.pointerUp(backdrop, { clientX: 60, clientY: 200 });
    expect(onClose).not.toHaveBeenCalled();
    // Wisch beginnt im Dialog, endet auf dem Hintergrund
    fireEvent.pointerDown(screen.getByText("Inhalt"), { clientX: 5, clientY: 5 });
    fireEvent.pointerUp(backdrop, { clientX: 6, clientY: 6 });
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.pointerDown(backdrop, { clientX: 5, clientY: 5 });
    fireEvent.pointerUp(backdrop, { clientX: 7, clientY: 6 });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
