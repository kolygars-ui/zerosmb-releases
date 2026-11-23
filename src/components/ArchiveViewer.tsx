import { useEffect, useMemo, useRef, useState } from "react";
import * as pdfjsLib from "pdfjs-dist";
import pdfWorker from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { api, formatBytes, kindOfName, mimeFor } from "../api";
import type { ArchiveEntry } from "../types";

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorker;

interface Props {
  open: boolean;
  path: string;
  name: string;
  externalDocuments: boolean;
  onClose: () => void;
}

type Preview =
  | { kind: "text"; text: string }
  | { kind: "image" | "video"; url: string }
  | { kind: "pdf"; pages: string[]; total: number }
  | null;

interface ArchiveRow {
  entry: ArchiveEntry;
  path: string;
  name: string;
  isDir: boolean;
}

function archivePath(value: string) {
  return value.replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
}

function parentArchivePath(value: string) {
  const path = archivePath(value);
  const index = path.lastIndexOf("/");
  return index < 0 ? "" : path.slice(0, index);
}

function PdfPreview({ buffer }: { buffer: ArrayBuffer }) {
  const [pages, setPages] = useState<string[]>([]);
  const [total, setTotal] = useState(0);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    let documentProxy: { destroy: () => Promise<void> } | null = null;
    void (async () => {
      try {
        const pdf = await pdfjsLib.getDocument({
          data: new Uint8Array(buffer.slice(0)),
        }).promise;
        documentProxy = pdf;
        if (cancelled) return;
        setTotal(pdf.numPages);
        const rendered: string[] = [];
        for (let index = 1; index <= Math.min(pdf.numPages, 20); index += 1) {
          const page = await pdf.getPage(index);
          const viewport = page.getViewport({ scale: 1.25 });
          const canvas = document.createElement("canvas");
          canvas.width = Math.ceil(viewport.width);
          canvas.height = Math.ceil(viewport.height);
          const context = canvas.getContext("2d");
          if (!context) continue;
          await page.render({ canvasContext: context, viewport }).promise;
          rendered.push(canvas.toDataURL("image/png"));
          if (cancelled) return;
          setPages([...rendered]);
        }
      } catch (cause) {
        if (!cancelled) setError(String(cause));
      }
    })();
    return () => {
      cancelled = true;
      void documentProxy?.destroy();
    };
  }, [buffer]);

  if (error) return <div className="error">{error}</div>;
  if (pages.length === 0) return <div className="empty">Загрузка PDF…</div>;
  return (
    <div className="pdf-preview">
      {pages.map((page, index) => (
        <img key={index} src={page} alt={`Страница ${index + 1}`} />
      ))}
      {total > pages.length && (
        <div className="hint">Показаны первые {pages.length} из {total} страниц</div>
      )}
    </div>
  );
}

export default function ArchiveViewer({
  open,
  path,
  name,
  externalDocuments,
  onClose,
}: Props) {
  const [entries, setEntries] = useState<ArchiveEntry[]>([]);
  const [currentDir, setCurrentDir] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [note, setNote] = useState("");
  const [selected, setSelected] = useState<ArchiveEntry | null>(null);
  const [preview, setPreview] = useState<Preview>(null);
  const [pdfBuffer, setPdfBuffer] = useState<ArrayBuffer | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const objectUrl = useRef("");

  useEffect(() => {
    if (!open) return;
    setEntries([]);
    setCurrentDir("");
    setFullscreen(false);
    setSelected(null);
    setPreview(null);
    setPdfBuffer(null);
    setError("");
    setNote("");
    setLoading(true);
    void api
      .archiveList(path)
      .then((list) => {
        setEntries(
          [...list].sort((a, b) => {
            if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
            return a.name.localeCompare(b.name, "ru");
          }),
        );
      })
      .catch((cause) => setError(String(cause)))
      .finally(() => setLoading(false));
    return () => {
      if (objectUrl.current) URL.revokeObjectURL(objectUrl.current);
      objectUrl.current = "";
    };
  }, [open, path]);

  const rows = useMemo(() => {
    const result = new Map<string, ArchiveRow>();
    const base = currentDir ? `${currentDir}/` : "";
    const lowerBase = base.toLowerCase();
    for (const entry of entries) {
      const full = archivePath(entry.name);
      if (!full) continue;
      let relative = full;
      if (base) {
        if (!full.toLowerCase().startsWith(lowerBase)) continue;
        relative = full.slice(base.length);
      }
      relative = relative.replace(/^\/+/, "");
      if (!relative) continue;
      const slash = relative.indexOf("/");
      if (slash >= 0) {
        const name = relative.slice(0, slash);
        const path = base ? `${currentDir}/${name}` : name;
        if (!result.has(path)) {
          result.set(path, {
            entry: { ...entry, index: -1, name: path, isDir: true, size: 0 },
            path,
            name,
            isDir: true,
          });
        }
      } else {
        const path = base ? `${currentDir}/${relative}` : relative;
        result.set(path, { entry, path, name: relative, isDir: entry.isDir });
      }
    }
    return [...result.values()].sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      return a.name.localeCompare(b.name, "ru");
    });
  }, [entries, currentDir]);

  if (!open) return null;

  const clearUrl = () => {
    if (objectUrl.current) {
      URL.revokeObjectURL(objectUrl.current);
      objectUrl.current = "";
    }
  };

  const openEntry = async (row: ArchiveRow) => {
    const entry = row.entry;
    if (row.isDir) {
      setCurrentDir(row.path);
      setFullscreen(false);
      setSelected(null);
      setPreview(null);
      setPdfBuffer(null);
      setNote("");
      clearUrl();
      return;
    }
    setFullscreen(false);
    setSelected(entry);
    setPreview(null);
    setPdfBuffer(null);
    setNote("");
    clearUrl();
    const kind = kindOfName(entry.name);
    setPreviewBusy(true);
    try {
      if (kind === "document" && !externalDocuments) {
        const document = await api.archivePreviewEntry(path, entry.index);
        setPreview({ kind: "text", text: document.text });
      } else if (kind === "document" || kind === "archive") {
        await api.archiveOpenEntry(path, entry.index);
        setNote("Открыто в системном приложении");
      } else if (kind === "text") {
        const buffer = await api.archiveRead(path, entry.index);
        setPreview({
          kind: "text",
          text: new TextDecoder().decode(new Uint8Array(buffer)),
        });
      } else if (kind === "image" || kind === "gif" || kind === "video") {
        const buffer = await api.archiveRead(path, entry.index);
        const url = URL.createObjectURL(
          new Blob([buffer], { type: mimeFor(entry.name) }),
        );
        objectUrl.current = url;
        setPreview({ kind: kind === "video" ? "video" : "image", url });
      } else if (kind === "pdf") {
        const buffer = await api.archiveRead(path, entry.index);
        setPdfBuffer(buffer);
        setPreview({ kind: "pdf", pages: [], total: 0 });
      } else {
        await api.archiveOpenEntry(path, entry.index);
        setNote("Открыто в системном приложении");
      }
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPreviewBusy(false);
    }
  };

  const downloadEntry = async () => {
    if (!selected) return;
    setNote("Сохранение…");
    try {
      const saved = await api.archiveSaveEntry(path, selected.index);
      setNote(`Сохранено: ${saved}`);
    } catch (cause) {
      setNote(String(cause));
    }
  };

  const openEntryInSystem = async () => {
    if (!selected) return;
    try {
      await api.archiveOpenEntry(path, selected.index);
      setNote("Открыто в системном приложении");
    } catch (cause) {
      setNote(String(cause));
    }
  };

  return (
    <>
    <div
      className="confirm-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="confirm-card archive-card" role="dialog" aria-modal="true">
        <div className="row">
          <div>
            <h3 style={{ margin: 0 }}>{name}</h3>
            <span className="small muted">Архив · {entries.length} элементов</span>
          </div>
          <span className="spacer" />
          <button className="btn ghost" onClick={onClose}>
            Закрыть
          </button>
        </div>
        {error && <div className="error" style={{ marginTop: 10 }}>{error}</div>}
        {note && <div className="hint" style={{ marginTop: 10 }}>{note}</div>}
        <div className="archive-layout">
          <div className="archive-list">
            <div className="row archive-pathbar">
              <button
                className="btn ghost"
                onClick={() => {
                  setCurrentDir(parentArchivePath(currentDir));
                  setFullscreen(false);
                  setSelected(null);
                  setPreview(null);
                  setPdfBuffer(null);
                  setNote("");
                  clearUrl();
                }}
                disabled={!currentDir}
              >
                Назад
              </button>
              <span className="mono small" title={currentDir || "/"}>
                /{currentDir}
              </span>
            </div>
            {loading && <div className="empty">Открытие архива…</div>}
            {!loading && rows.length === 0 && <div className="empty">Папка пуста</div>}
            {!loading &&
              rows.map((row) => (
                <button
                  key={row.path}
                  className={`archive-row ${row.isDir ? "archive-dir" : ""} ${
                    !row.isDir && selected?.index === row.entry.index ? "active" : ""
                  }`}
                  onClick={() => void openEntry(row)}
                  title={row.path}
                >
                  <span className="archive-icon">{row.isDir ? "DIR" : "FILE"}</span>
                  <span className="archive-name">{row.name}</span>
                  <span className="archive-size">
                    {row.isDir ? "" : formatBytes(row.entry.size)}
                  </span>
                </button>
              ))}
          </div>
          <div className="archive-preview">
            {!selected && <div className="empty">Выберите файл в архиве</div>}
            {selected && previewBusy && <div className="empty">Загрузка…</div>}
            {selected && !previewBusy && preview?.kind === "text" && (
              <pre className="text-preview">{preview.text || "(файл пуст)"}</pre>
            )}
            {selected && !previewBusy && preview?.kind === "image" && (
              <img src={preview.url} alt={selected.name} />
            )}
            {selected && !previewBusy && preview?.kind === "video" && (
              <video src={preview.url} controls autoPlay playsInline />
            )}
            {selected && !previewBusy && preview?.kind === "pdf" && pdfBuffer && (
              <PdfPreview buffer={pdfBuffer} />
            )}
            {selected && !previewBusy && !preview && (
              <div className="empty">Нет встроенного предпросмотра</div>
            )}
          </div>
        </div>
        {selected && !selected.isDir && (
          <div className="row end" style={{ marginTop: 12 }}>
            {(preview || pdfBuffer) && (
              <button className="btn" onClick={() => setFullscreen(true)}>
                На весь экран
              </button>
            )}
            <button className="btn" onClick={() => void openEntryInSystem()}>
              Открыть в системе
            </button>
            <button className="btn primary" onClick={() => void downloadEntry()}>
              Скачать файл
            </button>
          </div>
        )}
      </div>
    </div>
    {fullscreen && selected && (
      <div
        className="archive-fullscreen"
        onMouseDown={(event) => {
          if (event.target === event.currentTarget) setFullscreen(false);
        }}
      >
        <div className="archive-fullscreen-head">
          <strong>{selected.name}</strong>
          <button className="btn ghost" onClick={() => setFullscreen(false)}>
            Закрыть
          </button>
        </div>
        <div className="archive-fullscreen-stage">
          {preview?.kind === "text" && (
            <pre className="text-preview">{preview.text || "(файл пуст)"}</pre>
          )}
          {preview?.kind === "image" && <img src={preview.url} alt={selected.name} />}
          {preview?.kind === "video" && (
            <video src={preview.url} controls autoPlay playsInline />
          )}
          {preview?.kind === "pdf" && pdfBuffer && <PdfPreview buffer={pdfBuffer} />}
        </div>
      </div>
    )}
    </>
  );
}
