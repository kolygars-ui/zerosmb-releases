import { useEffect, useState } from "react";
import { api } from "../api";
import type { FileEntry } from "../types";

interface Props {
  open: boolean;
  title: string;
  onSelect: (path: string) => void;
  onCancel: () => void;
}

export default function FolderPickerDialog({ open, title, onSelect, onCancel }: Props) {
  const [path, setPath] = useState("");
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!open) return;
    setPath("");
    setEntries([]);
    setError("");
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let disposed = false;
    setLoading(true);
    setError("");
    void api
      .smbList(path)
      .then((list) => {
        if (disposed) return;
        setEntries(
          list
            .filter((entry) => entry.isDir)
            .sort((a, b) => a.name.localeCompare(b.name, "ru")),
        );
      })
      .catch((cause) => {
        if (!disposed) setError(String(cause));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [open, path]);

  if (!open) return null;

  const parent = path.includes("/")
    ? path.slice(0, path.lastIndexOf("/"))
    : "";

  return (
    <div
      className="confirm-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
    >
      <div className="confirm-card folder-pick-card" role="dialog" aria-modal="true">
        <h3>{title}</h3>
        <p>Выберите папку на NAS, которую нужно запретить для ИИ-агентов.</p>
        <div className="row">
          <button className="btn" onClick={() => setPath(parent)} disabled={!path}>
            Вверх
          </button>
          <span className="folder-pick-path mono">/{path || ""}</span>
        </div>
        <div className="folder-pick-list">
          {loading && <div className="empty">Загрузка…</div>}
          {!loading && error && <div className="error">{error}</div>}
          {!loading && !error && entries.length === 0 && (
            <div className="hint">В этой папке нет вложенных папок</div>
          )}
          {!loading &&
            !error &&
            entries.map((entry) => (
              <button
                key={entry.path}
                className="folder-pick-row"
                onClick={() => setPath(entry.path)}
              >
                <span className="folder-pick-icon">DIR</span>
                <span>{entry.name}</span>
              </button>
            ))}
        </div>
        <div className="row end" style={{ marginTop: 14 }}>
          <button className="btn" onClick={onCancel}>
            Отмена
          </button>
          <button
            className="btn primary"
            onClick={() => onSelect(path)}
            disabled={!path || loading}
          >
            Запретить эту папку
          </button>
        </div>
      </div>
    </div>
  );
}
