import type { ReactNode } from "react";

interface Props {
  open: boolean;
  title: string;
  message: ReactNode;
  confirmLabel?: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  children?: ReactNode;
}

export default function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "Удалить",
  busy = false,
  onConfirm,
  onCancel,
  children,
}: Props) {
  if (!open) return null;
  return (
    <div
      className="confirm-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onCancel();
      }}
    >
      <div className="confirm-card" role="dialog" aria-modal="true">
        <h3>{title}</h3>
        <p>{message}</p>
        {children}
        <div className="row end">
          <button className="btn" onClick={onCancel} disabled={busy}>
            Отмена
          </button>
          <button className="btn danger" onClick={onConfirm} disabled={busy}>
            {busy ? "Удаление…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
