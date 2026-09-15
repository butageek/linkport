// Transient autosave confirmation rendered next to a page title: appears
// for ~2s after every successful save (driven by useConfigEditor.justSaved).
export function SavedIndicator({ show }: { show: boolean }) {
  if (!show) return null;
  return (
    <span
      role="status"
      className="text-sm font-medium text-emerald-600 dark:text-emerald-400"
    >
      ✓ Saved
    </span>
  );
}
