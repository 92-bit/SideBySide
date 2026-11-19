// Keeps the keyboard inside a dialog while it is open:
// - Tab and Shift+Tab move between the dialog's buttons and boxes only;
// - Escape closes it (when `onEscape` allows), wherever the focus is;
// - the app's shortcuts (Ctrl+O, Ctrl+F, F7/F8, …) don't act on the screen behind it.
// When the dialog closes, the focus goes back to where it was.

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function trapFocus(node: HTMLElement, params: { onEscape: () => void }) {
  let onEscape = params.onEscape;
  const before = document.activeElement as HTMLElement | null;
  if (!node.contains(document.activeElement)) node.focus();

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onEscape();
    } else if (e.key === 'Tab') {
      const items = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
      if (items.length === 0) {
        e.preventDefault();
        node.focus();
      } else {
        const first = items[0];
        const last = items[items.length - 1];
        const inside = node.contains(document.activeElement);
        if (e.shiftKey && (!inside || document.activeElement === first || document.activeElement === node)) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && (!inside || document.activeElement === last)) {
          e.preventDefault();
          first.focus();
        }
      }
    } else if ((e.ctrlKey || e.metaKey) && !['c', 'a', 'x', 'v', 'z'].includes(e.key.toLowerCase())) {
      e.preventDefault(); // no app or browser shortcuts while the dialog is open
    }
    // Keys still do their normal job inside the dialog (Enter or Space on a button, typing),
    // but the rest of the app doesn't see them.
    e.stopPropagation();
  }

  window.addEventListener('keydown', onKey, true);
  return {
    update(p: { onEscape: () => void }) {
      onEscape = p.onEscape;
    },
    destroy() {
      window.removeEventListener('keydown', onKey, true);
      if (before && document.contains(before)) before.focus();
    },
  };
}
