import { useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  formatBytes,
  kindOfName,
  mediaUrl,
  mimeFor,
} from "../api";
import type {
  DownloadResult,
  FileEntry,
  GalleryItem,
  SyncProgress,
  SyncSummary,
  ViewerItem,
} from "../types";
import ConfirmDialog from "./ConfirmDialog";

type SortMode = "date-desc" | "date-asc" | "name-asc" | "name-desc";

interface Props {
  android: boolean;
  remoteGalleryPath: string;
  suggestedGalleryPath: string;
  galleryColumns: number;
  backToken: number;
  onPathChange: (path: string, canGoBack: boolean) => void;
  onOpenViewer: (item: ViewerItem, items?: ViewerItem[], index?: number) => void;
  refreshToken: number;
}

function PhoneCard({
  item,
  selected,
  onToggle,
}: {
  item: GalleryItem;
  selected: boolean;
  onToggle: (item: GalleryItem) => void;
}) {
  const ref = useRef<HTMLButtonElement | null>(null);
  const [thumb, setThumb] = useState("");
  const [failed, setFailed] = useState(false);
  const isVideo = item.mime.startsWith("video/");

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    let cancelled = false;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting) return;
        observer.disconnect();
        void api
          .galleryThumbnail(item.uri, 320)
          .then((url) => {
            if (!cancelled) setThumb(url);
          })
          .catch(() => {
            if (!cancelled) setFailed(true);
          });
      },
      { rootMargin: "80px" },
    );
    observer.observe(element);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [item.uri]);

  return (
    <button
      className={`media-card ${selected ? "selected" : ""}`}
      ref={ref}
      onClick={() => onToggle(item)}
      title={`${item.bucket || "Без альбома"} / ${item.name}`}
    >
      {thumb ? (
        <img src={thumb} alt={item.name} />
      ) : (
        <div className="no-thumb">{failed ? "IMG" : isVideo ? "VID" : "…"}</div>
      )}
      <span className="select-mark">{selected ? "✓" : ""}</span>
      {isVideo && <span className="badge">VIDEO</span>}
      <span className="caption">{item.name}</span>
    </button>
  );
}

function NasCard({
  entry,
  onOpen,
  onDownload,
  selectMode,
  selected,
  onSelect,
}: {
  entry: FileEntry;
  onOpen: (entry: FileEntry) => void;
  onDownload: (entry: FileEntry) => void;
  selectMode: boolean;
  selected: boolean;
  onSelect: (entry: FileEntry) => void;
}) {
  const ref = useRef<HTMLButtonElement | null>(null);
  const [src, setSrc] = useState("");
  const [failed, setFailed] = useState(false);
  const kind = kindOfName(entry.name);
  const isImage = kind === "image" || kind === "gif";

  useEffect(() => {
    const element = ref.current;
    if (!element || !isImage) return;
    let cancelled = false;
    let objectUrl = "";
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting) return;
        observer.disconnect();
        void api
          .readMediaBytes(entry.path)
          .catch(() => api.readFileBytes(entry.path))
          .then((buffer) => {
            if (cancelled) return;
            objectUrl = URL.createObjectURL(new Blob([buffer], { type: mimeFor(entry.name) }));
            setSrc(objectUrl);
          })
          .catch(() => {
            if (!cancelled) setFailed(true);
          });
      },
      { rootMargin: "80px" },
    );
    observer.observe(element);
    return () => {
      cancelled = true;
      observer.disconnect();
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [entry.path, isImage]);

  return (
    <button
      className={`media-card ${selected ? "selected" : ""}`}
      ref={ref}
      onClick={() => (selectMode ? onSelect(entry) : onOpen(entry))}
      title={entry.path}
    >
      {src && !failed ? (
        <img
          src={src}
          alt=""
          loading="lazy"
          decoding="async"
          onError={() => setFailed(true)}
        />
      ) : (
        <div className="no-thumb">{isImage ? "IMG" : "VID"}</div>
      )}
      {selectMode && (
        <span className="select-mark">{selected ? "✓" : ""}</span>
      )}
      {kind === "video" && <span className="badge">VIDEO</span>}
      <span
        className="card-action"
        role="button"
        tabIndex={0}
        title="Скачать"
        onClick={(event) => {
          event.stopPropagation();
          onDownload(entry);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.stopPropagation();
            onDownload(entry);
          }
        }}
      >
        ↓
      </span>
      <span className="caption">
        {entry.name} · {formatBytes(entry.size)}
      </span>
    </button>
  );
}
export default function GalleryView({
  android,
  remoteGalleryPath,
  suggestedGalleryPath,
  galleryColumns,
  backToken,
  onPathChange,
  onOpenViewer,
  refreshToken,
}: Props) {
  const [mode, setMode] = useState<"nas" | "phone">("nas");
  const [rootPath, setRootPath] = useState(remoteGalleryPath || "Gallery");
  const [nasPath, setNasPath] = useState(remoteGalleryPath || "Gallery");
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loadingNas, setLoadingNas] = useState(false);
  const [error, setError] = useState("");
  const [items, setItems] = useState<GalleryItem[]>([]);
  const [loadingPhone, setLoadingPhone] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [album, setAlbum] = useState("__all");
  const [sort, setSort] = useState<SortMode>("date-desc");
  const [conflictPolicy, setConflictPolicy] = useState<"skip" | "rename">("skip");
  const [syncPath, setSyncPath] = useState(remoteGalleryPath || "Gallery");
  const [pathInput, setPathInput] = useState(remoteGalleryPath || "Gallery");
  const [progress, setProgress] = useState<SyncProgress | null>(null);
  const [summary, setSummary] = useState<SyncSummary | null>(null);
  const [backupBusy, setBackupBusy] = useState(false);
  const [downloadBusy, setDownloadBusy] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<SyncProgress | null>(null);
  const [downloadSummary, setDownloadSummary] = useState<DownloadResult | null>(null);
  const [uploadBusy, setUploadBusy] = useState(false);
  const [downloadConflict, setDownloadConflict] = useState<"skip" | "rename">("skip");
  const [nasSelectMode, setNasSelectMode] = useState(false);
  const [selectedNas, setSelectedNas] = useState<Set<string>>(new Set());
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [itemNote, setItemNote] = useState("");

  useEffect(() => {
    const root = remoteGalleryPath || "Gallery";
    setRootPath(root);
    setNasPath(root);
    setSyncPath(root);
    setPathInput(root);
    void (async () => {
      const ok = await loadNasOrCreate(root);
      if (!ok && suggestedGalleryPath && suggestedGalleryPath !== root) {
        setRootPath(suggestedGalleryPath);
        setSyncPath(suggestedGalleryPath);
        await loadNasOrCreate(suggestedGalleryPath);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [remoteGalleryPath]);

  const loadNas = async (path: string): Promise<boolean> => {
    setLoadingNas(true);
    setError("");
    try {
      const list = await api.smbList(path);
      setEntries(list);
      setNasPath(path);
      setPathInput(path);
      return true;
    } catch (cause) {
      setError(String(cause));
      return false;
    } finally {
      setLoadingNas(false);
    }
  };

  const loadNasOrCreate = async (path: string): Promise<boolean> => {
    if (await loadNas(path)) return true;
    try {
      const created = await api.ensureGalleryFolder(path);
      if (created) setItemNote(`Папка галереи создана: ${path}`);
      return await loadNas(path);
    } catch (cause) {
      setError(String(cause));
      return false;
    }
  };

  const loadPhone = async () => {
    if (!android) return;
    setLoadingPhone(true);
    setError("");
    try {
      const response = await api.galleryList();
      setItems(response.items);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoadingPhone(false);
    }
  };

  useEffect(() => {
    if (refreshToken > 0) {
      if (mode === "nas") void loadNas(nasPath);
      else void loadPhone();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshToken]);

  useEffect(() => {
    if (mode === "phone" && items.length === 0) void loadPhone();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  const albums = useMemo(
    () =>
      Array.from(new Set(items.map((item) => item.bucket || "Без альбома"))).sort(
        (a, b) => a.localeCompare(b),
      ),
    [items],
  );

  const visiblePhoneItems = useMemo(() => {
    const list = items.filter(
      (item) => album === "__all" || (item.bucket || "Без альбома") === album,
    );
    list.sort((a, b) => {
      if (sort === "name-asc") return a.name.localeCompare(b.name, "ru");
      if (sort === "name-desc") return b.name.localeCompare(a.name, "ru");
      const aDate = a.dateCreated || a.dateModified;
      const bDate = b.dateCreated || b.dateModified;
      return sort === "date-asc" ? aDate - bDate : bDate - aDate;
    });
    return list;
  }, [album, items, sort]);

  const nasFolders = useMemo(
    () =>
      entries
        .filter((entry) => entry.isDir)
        .sort((a, b) => {
          if (sort === "name-asc") return a.name.localeCompare(b.name, "ru");
          if (sort === "name-desc") return b.name.localeCompare(a.name, "ru");
          return sort === "date-asc" ? a.modified - b.modified : b.modified - a.modified;
        }),
    [entries, sort],
  );

  const nasMedia = useMemo(
    () =>
      entries
        .filter((entry) => {
          if (entry.isDir) return false;
          const kind = kindOfName(entry.name);
          return kind === "image" || kind === "gif" || kind === "video";
        })
        .sort((a, b) => {
          if (sort === "name-asc") return a.name.localeCompare(b.name, "ru");
          if (sort === "name-desc") return b.name.localeCompare(a.name, "ru");
          return sort === "date-asc" ? a.modified - b.modified : b.modified - a.modified;
        }),
    [entries, sort],
  );

  const segments = nasPath.split("/").filter(Boolean);
  const galleryRoot = (rootPath || "Gallery").split("/").filter(Boolean);

  useEffect(() => {
    if (mode !== "nas") {
      onPathChange("", false);
      return;
    }
    const rootDepth = rootPath.split("/").filter(Boolean).length;
    const pathDepth = nasPath.split("/").filter(Boolean).length;
    onPathChange(nasPath, pathDepth > rootDepth);
  }, [mode, nasPath, rootPath, onPathChange]);

  useEffect(() => {
    if (backToken <= 0 || mode !== "nas") return;
    const rootDepth = rootPath.split("/").filter(Boolean).length;
    const segments = nasPath.split("/").filter(Boolean);
    if (segments.length > rootDepth) {
      void loadNas(segments.slice(0, -1).join("/"));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [backToken]);
  const openRoot = (path: string) => {
    const next = path.trim() || suggestedGalleryPath || "Gallery";
    setRootPath(next);
    setSyncPath(next);
    void loadNasOrCreate(next);
  };
  const openNasEntry = (entry: FileEntry) => {
    if (entry.isDir) {
      void loadNas(entry.path);
      return;
    }
    const kind = kindOfName(entry.name);
    if (kind !== "image" && kind !== "gif" && kind !== "video") return;
    const items: ViewerItem[] = nasMedia.map((file) => ({
      url: mediaUrl(`smb/${file.path}`),
      remotePath: file.path,
      name: file.name,
      kind: kindOfName(file.name),
    }));
    const index = nasMedia.findIndex((file) => file.path === entry.path);
    onOpenViewer(
      {
        url: mediaUrl(`smb/${entry.path}`),
        remotePath: entry.path,
        name: entry.name,
        kind,
      },
      items,
      Math.max(index, 0),
    );
  };

  const toggle = (item: GalleryItem) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(item.uri)) next.delete(item.uri);
      else next.add(item.uri);
      return next;
    });
  };

  const selectVisible = () => {
    setSelected((current) => {
      const next = new Set(current);
      for (const item of visiblePhoneItems) next.add(item.uri);
      return next;
    });
  };

  const downloadOne = async (entry: FileEntry) => {
    setError("");
    setItemNote("");
    try {
      const relative = entry.path.startsWith(`${rootPath}/`)
        ? entry.path.slice(rootPath.length + 1)
        : entry.name;
      const parent = relative.includes("/")
        ? relative.slice(0, relative.lastIndexOf("/"))
        : "";
      const saved = await api.downloadFile(
        entry.path,
        parent,
        downloadConflict,
      );
      setItemNote(`Сохранено: ${saved}`);
    } catch (cause) {
      setError(String(cause));
    }
  };

  const uploadToGallery = async () => {
    setUploadBusy(true);
    setError("");
    setItemNote("");
    try {
      await api.ensureGalleryFolder(nasPath);
      const result = await api.uploadFiles(nasPath);
      setItemNote(
        `Загружено: ${result.uploaded}, ошибок: ${result.failed}${
          result.errors.length ? `\n${result.errors.slice(0, 5).join("\n")}` : ""
        }`,
      );
      await loadNas(nasPath);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setUploadBusy(false);
    }
  };
  const toggleNas = (path: string) => {
    setSelectedNas((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const deleteSelected = async () => {
    if (selectedNas.size === 0) return;
    setDeleteBusy(true);
    setError("");
    try {
      const result = await api.deletePaths([...selectedNas]);
      setItemNote(`Удалено: ${result.deleted}, ошибок: ${result.failed}`);
      setSelectedNas(new Set());
      setNasSelectMode(false);
      await loadNas(nasPath);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setDeleteBusy(false);
    }
  };
  const downloadCurrent = async () => {
    setDownloadBusy(true);
    setError("");
    setDownloadSummary(null);
    setDownloadProgress(null);
    try {
      const result = await api.downloadGallery(
        nasPath,
        downloadConflict,
        setDownloadProgress,
      );
      setDownloadSummary(result);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setDownloadBusy(false);
    }
  };
  const sync = async () => {
    const chosen = items.filter((item) => selected.has(item.uri));
    if (chosen.length === 0) {
      setError("Выберите фотографии или папку для синхронизации");
      return;
    }
    setBackupBusy(true);
    setError("");
    setSummary(null);
    setProgress(null);
    try {
      const result = await api.syncGallery(
        syncPath.trim() || remoteGalleryPath || "Gallery",
        chosen,
        conflictPolicy,
        setProgress,
      );
      setSummary(result);
      await loadNas(remoteGalleryPath || "Gallery");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBackupBusy(false);
    }
  };

  return (
    <div className="stack">
      <div className="card">
        <div className="row">
          <div>
            <h3 style={{ marginBottom: 2 }}>Галерея</h3>
            <span className="small muted">
              {mode === "nas"
                ? "Фотографии, уже сохранённые на NAS"
                : `На телефоне: ${items.length} файлов · выбрано ${selected.size}`}
            </span>
          </div>
          <span className="spacer" />
          {android && (
            <button
              className={`btn ${mode === "nas" ? "primary" : ""}`}
              onClick={() => setMode("nas")}
            >
              На NAS
            </button>
          )}
          {android && (
            <button
              className={`btn ${mode === "phone" ? "primary" : ""}`}
              onClick={() => setMode("phone")}
            >
              Выбрать с телефона
            </button>
          )}
          {mode === "nas" && (
            <button
              className="btn primary"
              onClick={() => void uploadToGallery()}
              disabled={uploadBusy || loadingNas}
            >
              {uploadBusy ? "Загрузка…" : "Загрузить фото"}
            </button>
          )}
          {mode === "nas" && (
            <button
              className="btn"
              onClick={() => void downloadCurrent()}
              disabled={downloadBusy || loadingNas}
            >
              {downloadBusy ? "Скачивание…" : "Скачать папку"}
            </button>
          )}
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
          <button
            className="btn"
            onClick={() => {
              if (mode === "nas") void loadNas(nasPath);
              else void loadPhone();
            }}
            disabled={mode === "nas" ? loadingNas : loadingPhone}
          >
            Обновить
          </button>
        </div>
        {itemNote && (
          <div className="success" style={{ marginTop: 12, whiteSpace: "pre-wrap" }}>
            {itemNote}
          </div>
        )}
        {downloadProgress && (
          <div style={{ marginTop: 12 }}>
            <div className="progress">
              <div
                style={{
                  width: `${downloadProgress.total ? (downloadProgress.current / downloadProgress.total) * 100 : 0}%`,
                }}
              />
            </div>
            <div className="row small muted" style={{ marginTop: 6 }}>
              <span>
                {downloadProgress.current}/{downloadProgress.total}
              </span>
              <span className="spacer" />
              <span>{formatBytes(downloadProgress.bytes)}</span>
            </div>
            {downloadProgress.file && (
              <div className="hint mono">{downloadProgress.file}</div>
            )}
          </div>
        )}
        {downloadSummary && (
          <div className="success" style={{ marginTop: 12 }}>
            Скачано: {downloadSummary.downloaded}, пропущено:{" "}
            {downloadSummary.skipped}, ошибок: {downloadSummary.failed},{" "}
            {formatBytes(downloadSummary.bytes)} → {downloadSummary.destination}
            {downloadSummary.errors.length > 0 && (
              <div style={{ marginTop: 6 }}>
                {downloadSummary.errors.slice(0, 8).join("\n")}
              </div>
            )}
          </div>
        )}
        {error && <div className="error" style={{ marginTop: 12 }}>{error}</div>}
      </div>

      {mode === "phone" && (
        <div className="card">
          <div className="field">
            <label>Куда сохранять на NAS</label>
            <input
              value={syncPath}
              onChange={(event) => setSyncPath(event.target.value)}
              placeholder="Пользователь/Галерея"
              spellCheck={false}
            />
          </div>
          <div className="row" style={{ marginTop: 12 }}>
            <button
              className="btn"
              onClick={() => setSyncPath(suggestedGalleryPath)}
            >
              Стандартная папка
            </button>
            <div className="field" style={{ minWidth: 220 }}>
              <label>Если файл уже существует</label>
              <select
                value={conflictPolicy}
                onChange={(event) =>
                  setConflictPolicy(event.target.value as "skip" | "rename")
                }
              >
                <option value="skip">Пропустить</option>
                <option value="rename">Добавить (1), (2)…</option>
              </select>
            </div>
            <span className="spacer" />
            <button
              className="btn primary"
              onClick={() => void sync()}
              disabled={backupBusy || selected.size === 0}
            >
              {backupBusy ? "Синхронизация…" : "Синхронизировать выбранное"}
            </button>
          </div>
          <div className="hint" style={{ marginTop: 8 }}>
            Папки телефона сохраняются: скриншоты — в Screenshots, камера — в
            Camera и т.д. Выбранное правило применяется ко всем файлам.
          </div>
          {progress && (
            <div style={{ marginTop: 12 }}>
              <div className="progress">
                <div
                  style={{
                    width: `${progress.total ? (progress.current / progress.total) * 100 : 0}%`,
                  }}
                />
              </div>
              <div className="row small muted" style={{ marginTop: 6 }}>
                <span>
                  {progress.phase === "done"
                    ? "Готово"
                    : `${progress.current}/${progress.total}`}
                </span>
                <span className="spacer" />
                <span>
                  {formatBytes(progress.bytes)} · пропущено {progress.skipped} · ошибок{" "}
                  {progress.failed}
                </span>
              </div>
              {progress.file && <div className="hint mono">{progress.file}</div>}
            </div>
          )}
          {summary && (
            <div className="success" style={{ marginTop: 12 }}>
              Загружено: {summary.uploaded}, пропущено: {summary.skipped}, ошибок:{" "}
              {summary.failed}, объём: {formatBytes(summary.bytes)}
              {summary.errors.length > 0 && (
                <div style={{ marginTop: 6 }}>
                  {summary.errors.slice(0, 8).join("\n")}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {mode === "nas" ? (
        <>
          <div className="card">
            <div className="field">
              <label>Папка галереи на NAS</label>
              <input
                value={pathInput}
                onChange={(event) => setPathInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") openRoot(pathInput);
                }}
                placeholder="Пользователь/Галерея"
                spellCheck={false}
              />
            </div>
            <div className="row" style={{ marginTop: 12 }}>
              <button
                className="btn"
                onClick={() => {
                  const parts = nasPath.split("/").filter(Boolean);
                  const rootDepth = rootPath.split("/").filter(Boolean).length;
                  if (parts.length > rootDepth) {
                    void loadNas(parts.slice(0, -1).join("/"));
                  }
                }}
                disabled={
                  nasPath.split("/").filter(Boolean).length <=
                  rootPath.split("/").filter(Boolean).length
                }
              >
                Назад
              </button>
              <button className="btn" onClick={() => openRoot(rootPath)}>
                В корень
              </button>
              <button className="btn" onClick={() => openRoot(pathInput)}>
                Открыть
              </button>
              <button
                className="btn"
                onClick={() => openRoot(suggestedGalleryPath)}
              >
                Стандартная папка
              </button>
              <div className="field" style={{ minWidth: 220 }}>
                <label>При совпадении при скачивании</label>
                <select
                  value={downloadConflict}
                  onChange={(event) =>
                    setDownloadConflict(event.target.value as "skip" | "rename")
                  }
                >
                  <option value="skip">Пропустить</option>
                  <option value="rename">Добавить (1), (2)…</option>
                </select>
              </div>
              <span className="spacer" />
              <button
                className="btn"
                onClick={() => {
                  setNasSelectMode((value) => !value);
                  setSelectedNas(new Set());
                }}
              >
                {nasSelectMode ? "Отменить выбор" : "Выбрать для удаления"}
              </button>
              {nasSelectMode && selectedNas.size > 0 && (
                <button
                  className="btn danger"
                  onClick={() => setConfirmDelete(true)}
                  disabled={deleteBusy}
                >
                  {deleteBusy
                    ? "Удаление…"
                    : `Удалить выбранное (${selectedNas.size})`}
                </button>
              )}
            </div>
            <div className="breadcrumbs" style={{ marginTop: 8 }}>
              <button onClick={() => openRoot(rootPath)}>
                Галерея
              </button>
              {segments.slice(galleryRoot.length).map((segment, index) => {
                const target = segments
                  .slice(0, galleryRoot.length + index + 1)
                  .join("/");
                return (
                  <span key={target}>
                    / <button onClick={() => void loadNas(target)}>{segment}</button>
                  </span>
                );
              })}
            </div>
          </div>
          <div className="card">
            {loadingNas && entries.length === 0 && (
              <div className="empty">Загрузка галереи NAS…</div>
            )}
            {!loadingNas && nasFolders.length === 0 && nasMedia.length === 0 && (
              <div className="empty">
                В этой папке пока нет фотографий. Переключитесь на «Выбрать с
                телефона», отметьте альбом или снимки и синхронизируйте.
              </div>
            )}
            <div
              className="gallery-grid"
              style={{
                marginTop: 12,
                gridTemplateColumns: `repeat(${galleryColumns}, minmax(0, 1fr))`,
              }}
            >
              {nasFolders.map((entry) => (
                <button
                  key={entry.path}
                  className={`media-card folder-card ${
                    selectedNas.has(entry.path) ? "selected" : ""
                  }`}
                  onClick={() =>
                    nasSelectMode ? toggleNas(entry.path) : openNasEntry(entry)
                  }
                  title={entry.path}
                >
                  <div className="no-thumb">DIR</div>
                  {nasSelectMode && (
                    <span className="select-mark">
                      {selectedNas.has(entry.path) ? "✓" : ""}
                    </span>
                  )}
                  <span className="caption">{entry.name}</span>
                </button>
              ))}
              {nasMedia.map((entry) => (
                <NasCard
                  key={entry.path}
                  entry={entry}
                  onOpen={openNasEntry}
                  onDownload={(file) => void downloadOne(file)}
                  selectMode={nasSelectMode}
                  selected={selectedNas.has(entry.path)}
                  onSelect={(file) => toggleNas(file.path)}
                />
              ))}
            </div>
          </div>
        </>
      ) : (
        <>
          <div className="card">
            <div className="row">
              <div className="field" style={{ flex: 1, minWidth: 200 }}>
                <label>Альбом</label>
                <select value={album} onChange={(event) => setAlbum(event.target.value)}>
                  <option value="__all">Все альбомы</option>
                  {albums.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </div>
              <button className="btn" onClick={selectVisible}>
                Выбрать {album === "__all" ? "все" : "альбом"}
              </button>
              <button
                className="btn ghost"
                onClick={() => setSelected(new Set())}
                disabled={selected.size === 0}
              >
                Снять выбор
              </button>
              <span className="spacer" />
              <span className="chip mono">{selected.size} выбрано</span>
            </div>
            <div className="hint" style={{ marginTop: 8 }}>
              Файлы сохраняются в папку альбома на NAS:{" "}
              <span className="mono">{syncPath || remoteGalleryPath}</span>/<span className="mono">Альбом</span>
              . Имя файла и дата создания с телефона сохраняются.
            </div>
            {loadingPhone && items.length === 0 && (
              <div className="empty">Чтение галереи телефона…</div>
            )}
            {!loadingPhone && items.length === 0 && (
              <div className="empty">На телефоне не найдено фото или видео.</div>
            )}
          </div>

          {visiblePhoneItems.length > 0 && (
            <div
              className="gallery-grid"
              style={{
                gridTemplateColumns: `repeat(${galleryColumns}, minmax(0, 1fr))`,
              }}
            >
              {visiblePhoneItems.map((item) => (
                <PhoneCard
                  key={item.uri}
                  item={item}
                  selected={selected.has(item.uri)}
                  onToggle={toggle}
                />
              ))}
            </div>
          )}

        </>
      )}
      <ConfirmDialog
        open={confirmDelete}
        title="Удалить выбранные объекты?"
        message={`Будет удалено объектов: ${selectedNas.size}. Папки удаляются со всем содержимым.`}
        busy={deleteBusy}
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => {
          setConfirmDelete(false);
          void deleteSelected();
        }}
      />
    </div>
  );
}
