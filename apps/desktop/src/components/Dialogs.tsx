import { useState } from "react";
import { Modal } from "./Modal";

/** One text field: new collection, rename, add tag (§9.3). */
export function NameDialog({
  title,
  label,
  initial = "",
  confirm,
  max,
  suggestions,
  validate,
  onSubmit,
  onClose,
}: {
  title: string;
  label: string;
  initial?: string;
  confirm: string;
  max: number;
  /** Offered while typing (existing tag names). */
  suggestions?: string[];
  /** A message when the value can't be used (e.g. a name already taken). */
  validate?: (v: string) => string | null;
  onSubmit: (v: string) => Promise<void> | void;
  onClose: () => void;
}) {
  const [value, setValue] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const clean = value.trim().replace(/\s+/g, " ");
  const problem = !clean ? null : clean.length > max ? `At most ${max} characters.` : (validate?.(clean) ?? null);
  const listId = suggestions ? "name-suggestions" : undefined;
  return (
    <Modal title={title} onClose={onClose} testId="name-dialog">
      <form
        noValidate
        onSubmit={async (e) => {
          e.preventDefault();
          if (!clean || problem || busy) return;
          setBusy(true);
          try {
            await onSubmit(clean);
          } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
            setBusy(false);
          }
        }}
      >
        <label className="field">
          <span>{label}</span>
          <input type="text" value={value} onChange={(e) => setValue(e.target.value)} list={listId} aria-invalid={problem || error ? true : undefined} autoComplete="off" data-testid="name-input" />
        </label>
        {suggestions && (
          <datalist id={listId}>
            {suggestions.map((s) => (
              <option key={s} value={s} />
            ))}
          </datalist>
        )}
        {(problem || error) && (
          <p className="field-error small" role="alert" data-testid="name-error">
            {problem ?? error}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="button button-primary" disabled={!clean || !!problem || busy} data-testid="name-submit">
            {confirm}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export interface PickOption {
  value: string;
  label: string;
  disabled?: boolean;
}

/** Choose one: "Move to…", "Merge into…", "Add to collection…". */
export function PickDialog({
  title,
  label,
  options,
  confirm,
  note,
  onSubmit,
  onClose,
}: {
  title: string;
  label: string;
  options: PickOption[];
  confirm: string;
  note?: string;
  onSubmit: (value: string) => Promise<void> | void;
  onClose: () => void;
}) {
  const [value, setValue] = useState(options.find((o) => !o.disabled)?.value ?? "");
  const [busy, setBusy] = useState(false);
  return (
    <Modal title={title} onClose={onClose} testId="pick-dialog">
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (!value || busy) return;
          setBusy(true);
          try {
            await onSubmit(value);
          } finally {
            setBusy(false);
          }
        }}
      >
        <label className="field">
          <span>{label}</span>
          <select value={value} onChange={(e) => setValue(e.target.value)} data-testid="pick-select">
            {options.map((o) => (
              <option key={o.value} value={o.value} disabled={o.disabled}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        {note && <p className="muted small">{note}</p>}
        <div className="modal-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="button button-primary" disabled={!value || busy} data-testid="pick-submit">
            {confirm}
          </button>
        </div>
      </form>
    </Modal>
  );
}

/** Explicit consent for steps Undo cannot reverse (§7.3). */
export function ConfirmDialog({
  title,
  body,
  confirm,
  danger,
  checkbox,
  choices,
  onSubmit,
  onClose,
}: {
  title: string;
  body: React.ReactNode;
  confirm: string;
  danger?: boolean;
  /** An extra, off-by-default consent (e.g. also delete media files). */
  checkbox?: string;
  /** Radio choices; the first is the default. */
  choices?: PickOption[];
  onSubmit: (r: { checked: boolean; choice: string | null }) => Promise<void> | void;
  onClose: () => void;
}) {
  const [checked, setChecked] = useState(false);
  const [choice, setChoice] = useState(choices?.[0]?.value ?? null);
  const [busy, setBusy] = useState(false);
  return (
    <Modal title={title} onClose={onClose} testId="confirm-dialog">
      <div className="modal-body">{body}</div>
      {choices && (
        <fieldset className="choices">
          {choices.map((c) => (
            <label key={c.value} className="check">
              <input type="radio" name="choice" value={c.value} checked={choice === c.value} onChange={() => setChoice(c.value)} data-testid={`choice-${c.value}`} />
              {c.label}
            </label>
          ))}
        </fieldset>
      )}
      {checkbox && (
        <label className="check">
          <input type="checkbox" checked={checked} onChange={(e) => setChecked(e.target.checked)} data-testid="confirm-checkbox" />
          {checkbox}
        </label>
      )}
      <div className="modal-actions">
        <button type="button" className="button" onClick={onClose} data-autofocus>
          Cancel
        </button>
        <button
          type="button"
          className={`button ${danger ? "button-danger" : "button-primary"}`}
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              await onSubmit({ checked, choice });
            } finally {
              setBusy(false);
            }
          }}
          data-testid="confirm-submit"
        >
          {confirm}
        </button>
      </div>
    </Modal>
  );
}
