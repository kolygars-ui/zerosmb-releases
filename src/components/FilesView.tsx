import { useCallback, useEffect, useMemo, useState } from "react";
import { api, formatBytes, formatDate, kindOfName, mediaUrl } from "../api";
import type { DownloadResult, ExternalEditSession, FileEntry, SyncProgress, ViewerItem } from "../types";
import ConfirmDialog from "./ConfirmDialog";
import ArchiveViewer from "./ArchiveViewer";

interface Props {
  onOpenViewer: (item: ViewerItem) => void;
  refreshToken: number;
  backToken: number;
  openPath?: string;
  openPathToken?: number;
  android: boolean;
  viewMode: "list" | "details";
  onPathChange: (path: string) => void;
  onExternalEdit: (session: ExternalEditSession) => void;
  externalDocuments: boolean;
}

type SortMode = "date-desc" | "date-asc" | "name-asc" | "name-desc";

function iconFor(entry: FileEntry): string {
  if (entry.isDir) return "DIR";
  const kind = kindOfName(entry.name);
  if (kind === "video") return "VID";
  if (kind === "gif") return "GIF";
  if (kind === "image") return "IMG";
  if (kind === "pdf") return "PDF";
  if (kind === "text") return "TXT";
  return "FILE";
}

function typeLabel(entry: FileEntry): string {
  if (entry.isDir) return "Папка с файлами";
  const kind = kindOfName(entry.name);
  if (kind === "video") return "Видеофайл";
  if (kind === "gif") return "Изображение GIF";
  if (kind === "image") {
    const extension = entry.name.split(".").pop()?.toUpperCase();
    return extension ? `Изображение ${extension}` : "Изображение";
  }
  if (kind === "pdf") return "PDF-документ";
  if (kind === "text") return "Текстовый документ";
  return "Файл";
}

function parentOf(path: string): string {
  const segments = path.split("/").filter(Boolean);
  segments.pop();
  return segments.join("/");
}

export default function FilesView({
  onOpenViewer,
  refreshToken,
  backToken,
  openPath,
  openPathToken,
  android,
  viewMode,
  onPathChange,
  onExternalEdit,
  externalDocuments,
}: Props) {
  const [path, setPath] = useState("");
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [actionBusy, setActionBusy] = useState(false);
  const [selectMode, setSelectMode] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [archivePreview, setArchivePreview] = useState<{
    path: string;
    name: string;
  } | null>(null);
  const [sort, setSort] = useState<SortMode>("date-desc");
  const [folderBusy, setFolderBusy] = useState(false);
  const [folderProgress, setFolderProgress] = useState<SyncProgress | null>(null);
  const [folderResult, setFolderResult] = useState<DownloadResult | null>(null);

  const load = useCallback(async (nextPath: string) => {
    setLoading(true);
    setError("");
    try {
      const list = await api.smbList(nextPath);
      setEntries(list);
      setPath(nextPath);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load("");
  }, [load]);

  useEffect(() => {
    if (refreshToken > 0) void load(path);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshToken]);

  useEffect(() => {
    if (backToken > 0 && path) {
      void load(parentOf(path));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [backToken]);

  useEffect(() => {
    onPathChange(path);
  }, [path, onPathChange]);

  useEffect(() => {
    if (openPathToken && openPath !== undefined) {
      void load(openPath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openPathToken]);

  const sortedEntries = useMemo(() => {
    const list = [...entries];
    if (sort === "name-asc") {
      list.sort((a, b) => a.name.localeCompare(b.name, "ru"));
    } else if (sort === "name-desc") {
      list.sort((a, b) => b.name.localeCompare(a.name, "ru"));
    } else if (sort === "date-asc") {
      list.sort((a, b) => a.modified - b.modified);
    } else {
      list.sort((a, b) => b.modified - a.modified);
    }
    return list;
  }, [entries, sort]);

  const segments = path.split("/").filter(Boolean);

  const openEntry = (entry: FileEntry) => {
    if (selectMode) {
      setSelected((current) => {
        const next = new Set(current);
        if (next.has(entry.path)) next.delete(entry.path);
        else next.add(entry.path);
        return next;
      });
      return;
    }
    if (entry.isDir) {
      void load(entry.path);
      return;
    }
    const kind = kindOfName(entry.name);
    if (kind === "archive") {
      setArchivePreview({ path: entry.path, name: entry.name });
      return;
    }
    if (kind === "document" && !externalDocuments) {
      onOpenViewer({
        remotePath: entry.path,
        relativeDir: android ? undefined : parentOf(entry.path),
        name: entry.name,
        kind,
      });
      return;
    }
    if (kind === "file" || kind === "document") {
      setActionBusy(true);
      setError("");
      setMessage("");
      void api
        .openRemoteFile(entry.path)
        .then((file) => {
          setMessage(
            kind === "document"
              ? "Документ открыт во внешнем приложении."
              : "Открыто во внешнем приложении",
          );
          if (kind === "document") {
            onExternalEdit({
              remotePath: entry.path,
              localPath: file.localPath,
              name: entry.name,
              signature: file.signature,
            });
          }
        })
        .catch((cause) => setError(String(cause)))
        .finally(() => setActionBusy(false));
      return;
    }
    const relativeDir = android ? undefined : parentOf(entry.path);
    if (kind === "text" || kind === "pdf") {
      onOpenViewer({
        remotePath: entry.path,
        relativeDir,
        name: entry.name,
        kind,
      });
      return;
    }
    onOpenViewer({
      url: mediaUrl(`smb/${entry.path}`),
      remotePath: entry.path,
      relativeDir,
      name: entry.name,
      kind,
    });
  };

  const downloadFolder = async () => {
    setFolderBusy(true);
    setError("");
    setMessage("");
    setFolderResult(null);
    setFolderProgress(null);
    try {
      const result = await api.downloadGallery(path, "rename", setFolderProgress);
      setFolderResult(result);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setFolderBusy(false);
    }
  };
  const upload = async () => {
    setActionBusy(true);
    setError("");
    setMessage("");
    try {
      const result = await api.uploadFiles(path);
      setMessage(
        `Загружено: ${result.uploaded}, ошибок: ${result.failed}${
          result.errors.length ? `\n${result.errors.slice(0, 5).join("\n")}` : ""
        }`,
      );
      await load(path);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setActionBusy(false);
    }
  };

  const download = async (entry: FileEntry) => {
    setActionBusy(true);
    setError("");
    setMessage("");
    try {
      const saved = await api.downloadFile(
        entry.path,
        android ? undefined : parentOf(entry.path),
      );
      setMessage(`Сохранено: ${saved}`);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setActionBusy(false);
    }
  };

  const createFolder = async () => {
    const name = window.prompt("Имя новой папки");
    if (!name?.trim()) return;
    const clean = name.trim().replace(/^\/+|\/+$/g, "");
    if (!clean) return;
    const target = [path, clean].filter(Boolean).join("/");
    setActionBusy(true);
    setError("");
    setMessage("");
    try {
      await api.createFolder(target);
      setMessage(`Папка создана: ${clean}`);
      await load(path);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setActionBusy(false);
    }
  };

  const deleteSelected = async () => {
    if (selected.size === 0) return;
    setActionBusy(true);
    setError("");
    setMessage("");
    try {
      const result = await api.deletePaths([...selected]);
      setMessage(`Удалено: ${result.deleted}, ошибок: ${result.failed}`);
      setSelected(new Set());
      setSelectMode(false);
      await load(path);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setActionBusy(false);
    }
  };

  return (
    <div className="stack">
      <div className="card">
        <div className="row">
          <button
            className="btn"
            onClick={() => void load(parentOf(path))}
            disabled={!path || loading}
          >
            Назад
          </button>
          <button
            className="btn"
            onClick={() => void load("")}
            disabled={!path || loading}
          >
            В корень
          </button>
          <select
            className="sort-select"
            value={sort}
            onChange={(event) => setSort(event.target.value as SortMode)}
          >
            <option value="date-desc">Дата: новые сначала</option>
            <option value="date-asc">Дата: старые сначала</option>
            <option value="name-asc">Имя: А–Я</option>
            <option value="name-desc">Имя: Я–А</option>
          </select>
          <span className="spacer" />
          <button className="btn" onClick={() => void createFolder()} disabled={actionBusy || loading}>
            Создать папку
          </button>
          <button
            className="btn"
            onClick={() => {
              setSelectMode((value) => !value);
              setSelected(new Set());
            }}
          >
            {selectMode ? "Отменить выбор" : "Выбрать для удаления"}
          </button>
          {selectMode && selected.size > 0 && (
            <button
              className="btn danger"
              onClick={() => setConfirmDelete(true)}
              disabled={actionBusy}
            >
              Удалить выбранное ({selected.size})
            </button>
          )}
          <button className="btn" onClick={() => void downloadFolder()} disabled={folderBusy || loading}>
            {folderBusy ? "Скачивание…" : "Скачать папку"}
          </button>
          <button className="btn primary" onClick={() => void upload()} disabled={actionBusy || loading}>
            Загрузить файлы
          </button>
          <button
            className="btn"
            onClick={() => void load(path)}
            disabled={loading || actionBusy}
          >
            {loading ? "Загрузка…" : "Обновить"}
          </button>
        </div>
        {folderProgress && (
          <div style={{ marginTop: 10 }}>
            <div className="progress">
              <div
                style={{
                  width: `${folderProgress.total ? (folderProgress.current / folderProgress.total) * 100 : 0}%`,
                }}
              />
            </div>
            <div className="row small muted" style={{ marginTop: 5 }}>
              <span>
                {folderProgress.current}/{folderProgress.total}
              </span>
              <span className="spacer" />
              <span>{formatBytes(folderProgress.bytes)}</span>
            </div>
            {folderProgress.file && (
              <div className="hint mono">{folderProgress.file}</div>
            )}
          </div>
        )}
        {folderResult && (
          <div className="success" style={{ marginTop: 10 }}>
            Скачано: {folderResult.downloaded}, пропущено: {folderResult.skipped},
            ошибок: {folderResult.failed}, {formatBytes(folderResult.bytes)} →{" "}
            {folderResult.destination}
          </div>
        )}
        <div className="breadcrumbs" style={{ marginTop: 10 }}>
          <button onClick={() => void load("")}>Корень</button>
          {segments.map((segment, index) => {
            const target = segments.slice(0, index + 1).join("/");
            return (
              <span key={target}>
                / <button onClick={() => void load(target)}>{segment}</button>
              </span>
            );
          })}
        </div>
      </div>

      <div className="card" style={{ padding: 10 }}>
        {error && (
          <div className="error">
            {error}
            <div className="row end" style={{ marginTop: 8 }}>
              <button className="btn" onClick={() => void load(path)}>
                Повторить
              </button>
            </div>
          </div>
        )}
        {message && <div className="success" style={{ whiteSpace: "pre-wrap" }}>{message}</div>}
        {!error && entries.length === 0 && !loading && (
          <div className="empty">Папка пуста</div>
        )}
        {loading && entries.length === 0 && <div className="empty">Загрузка…</div>}
        {viewMode === "details" ? (
          <div className="details-wrap">
            <table className="details-table">
              <thead>
                <tr>
                  <th>Имя</th>
                  <th>Дата изменения</th>
                  <th>Тип</th>
                  <th>Размер</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {sortedEntries.map((entry) => {
                  const kind = kindOfName(entry.name);
                  const isPreview = !entry.isDir && kind !== "file";
                  const isSelected = selected.has(entry.path);
                  return (
                    <tr
                      key={entry.path}
                      className={`${isSelected ? "selected" : ""} ${entry.isDir ? "folder" : "file"}`}
                      onClick={() => openEntry(entry)}
                      title={entry.path}
                    >
                      <td>
                        <div className="details-name">
                          <div className="file-icon small">{iconFor(entry)}</div>
                          <strong>{entry.name}</strong>
                        </div>
                      </td>
                      <td className="details-meta">
                        {entry.modified ? formatDate(entry.modified) : ""}
                      </td>
                      <td className="details-meta">{typeLabel(entry)}</td>
                      <td className="details-meta">
                        {entry.isDir ? "" : formatBytes(entry.size)}
                      </td>
                      <td>
                        {selectMode ? (
                          <span className="chip">{isSelected ? "✓ выбран" : "выбрать"}</span>
                        ) : (
                          !entry.isDir && (
                            <div className="file-actions">
                              {isPreview && (
                                <button
                                  className="btn ghost file-action"
                                  title="Предпросмотр"
                                  onClick={(event) => {
                                    event.stopPropagation();
                                    openEntry(entry);
                                  }}
                                >
                                  {android ? "П" : "Просмотр"}
                                </button>
                              )}
                              <button
                                className="btn ghost file-action"
                                disabled={actionBusy}
                                onClick={(event) => {
                                  event.stopPropagation();
                                  void download(entry);
                                }}
                                title="Скачать файл"
                              >
                                Скачать
                              </button>
                            </div>
                          )
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="list">
            {sortedEntries.map((entry) => {
              const kind = kindOfName(entry.name);
              const isPreview = !entry.isDir && kind !== "file";
              const isSelected = selected.has(entry.path);
              return (
                <div
                  className={`file-row ${isSelected ? "selected" : ""} ${entry.isDir ? "folder" : "file"}`}
                  key={entry.path}
                  onClick={() => openEntry(entry)}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") openEntry(entry);
                  }}
                  style={{ cursor: "pointer" }}
                  title={entry.path}
                >
                  <div className="file-icon">{iconFor(entry)}</div>
                  <div className="file-title">
                    <strong>{entry.name}</strong>
                    <span>
                      {entry.isDir
                        ? "Папка"
                        : `${formatBytes(entry.size)}${entry.modified ? ` · ${formatDate(entry.modified)}` : ""}`}
                    </span>
                  </div>
                  {selectMode && (
                    <span className="chip">{isSelected ? "✓ выбран" : "выбрать"}</span>
                  )}
                  {!selectMode && !entry.isDir && (
                    <div className="file-actions">
                      {isPreview && (
                        <button
                          className="btn ghost file-action"
                          title="Предпросмотр"
                          onClick={(event) => {
                            event.stopPropagation();
                            openEntry(entry);
                          }}
                        >
                          {android ? "П" : "Просмотр"}
                        </button>
                      )}
                      <button
                        className="btn ghost file-action"
                        disabled={actionBusy}
                        onClick={(event) => {
                          event.stopPropagation();
                          void download(entry);
                        }}
                        title="Скачать файл"
                      >
                        Скачать
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
      <ArchiveViewer
        open={archivePreview !== null}
        path={archivePreview?.path ?? ""}
        name={archivePreview?.name ?? ""}
        externalDocuments={externalDocuments}
        onClose={() => setArchivePreview(null)}
      />
      <ConfirmDialog
        open={confirmDelete}
        title="Удалить выбранные объекты?"
        message={`Будет удалено объектов: ${selected.size}. Папки удаляются со всем содержимым.`}
        busy={actionBusy}
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => {
          setConfirmDelete(false);
          void deleteSelected();
        }}
      />
    </div>
  );
}
