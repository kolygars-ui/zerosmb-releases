import { useEffect, useRef, useState } from "react";
import * as pdfjsLib from "pdfjs-dist";
import pdfWorker from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { api, mimeFor } from "../api";
import type { ViewerItem } from "../types";

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorker;

interface Props {
  items: ViewerItem[];
  index: number;
  onIndexChange: (index: number) => void;
  onClose: () => void;
}

function TextViewer({ path }: { path: string }) {
  const [text, setText] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    setText("");
    setLoaded(false);
    setError("");
    void api
      .readTextFile(path)
      .then((value) => {
        if (!cancelled) {
          setText(value);
          setLoaded(true);
        }
      })
      .catch((cause) => {
        if (!cancelled) setError(String(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  if (error) return <div className="error">{error}</div>;
  if (!loaded) return <div className="empty">Загрузка текста…</div>;
  return <pre className="text-preview">{text || "(файл пуст)"}</pre>;
}

function DocumentViewer({ path }: { path: string }) {
  const [text, setText] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    setText("");
    setLoaded(false);
    setError("");
    void api
      .previewDocument(path)
      .then((preview) => {
        if (!cancelled) {
          setText(preview.text);
          setLoaded(true);
        }
      })
      .catch((cause) => {
        if (!cancelled) setError(String(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  if (error) return <div className="error">{error}</div>;
  if (!loaded) return <div className="empty">Загрузка документа…</div>;
  return <pre className="text-preview">{text || "(текст не найден)"}</pre>;
}

function PdfViewer({ path }: { path: string }) {
  const [pages, setPages] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [total, setTotal] = useState(0);

  useEffect(() => {
    let cancelled = false;
    let documentProxy: { destroy: () => Promise<void> } | null = null;
    setPages([]);
    setError("");
    setTotal(0);

    void (async () => {
      try {
        const buffer = await api.readFileBytes(path);
        const pdf = await pdfjsLib.getDocument({
          data: new Uint8Array(buffer),
        }).promise;
        documentProxy = pdf;
        if (cancelled) return;
        setTotal(pdf.numPages);
        const limit = Math.min(pdf.numPages, 30);
        const rendered: string[] = [];
        for (let pageIndex = 1; pageIndex <= limit; pageIndex += 1) {
          const page = await pdf.getPage(pageIndex);
          const viewport = page.getViewport({ scale: 1.35 });
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
  }, [path]);

  if (error) return <div className="error">{error}</div>;
  if (pages.length === 0) return <div className="empty">Загрузка PDF…</div>;
  return (
    <div className="pdf-preview">
      {pages.map((page, pageIndex) => (
        <img key={pageIndex} src={page} alt={`Страница ${pageIndex + 1}`} />
      ))}
      {total > pages.length && (
        <div className="hint">
          Показаны первые {pages.length} из {total} страниц. Скачайте файл, чтобы
          посмотреть полностью.
        </div>
      )}
    </div>
  );
}

function ImageViewer({ item }: { item: ViewerItem }) {
  const [src, setSrc] = useState(item.url ?? "");
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    let objectUrl = "";
    setError("");
    setSrc(item.url ?? "");
    if (!item.remotePath) return;
    void api
      .readMediaBytes(item.remotePath)
      .catch(() => api.readFileBytes(item.remotePath!))
      .then((buffer) => {
        if (cancelled) return;
        objectUrl = URL.createObjectURL(new Blob([buffer], { type: mimeFor(item.name) }));
        setSrc(objectUrl);
      })
      .catch(() => {
        if (!cancelled) setSrc(item.url ?? "");
      });
    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [item.remotePath, item.url]);

  if (error) return <div className="error">{error}</div>;
  if (!src) return <div className="empty">Загрузка изображения…</div>;
  return (
    <img
      src={src}
      alt={item.name}
      onError={() => setError("Не удалось загрузить изображение")}
    />
  );
}

function VideoViewer({ item }: { item: ViewerItem }) {
  const [src, setSrc] = useState(item.url ?? "");
  const [triedBytes, setTriedBytes] = useState(false);
  const [error, setError] = useState("");
  const objectUrl = useRef("");

  useEffect(() => {
    setSrc(item.url ?? "");
    setTriedBytes(false);
    setError("");
    if (objectUrl.current) {
      URL.revokeObjectURL(objectUrl.current);
      objectUrl.current = "";
    }
    return () => {
      if (objectUrl.current) {
        URL.revokeObjectURL(objectUrl.current);
        objectUrl.current = "";
      }
    };
  }, [item.url, item.remotePath]);

  const fallback = async () => {
    if (triedBytes || !item.remotePath) {
      setError("Не удалось воспроизвести видео");
      return;
    }
    setTriedBytes(true);
    try {
      const buffer = await api.readMediaBytes(item.remotePath);
      objectUrl.current = URL.createObjectURL(
        new Blob([buffer], { type: mimeFor(item.name) }),
      );
      setSrc(objectUrl.current);
    } catch {
      setError("Не удалось воспроизвести видео");
    }
  };

  if (error) return <div className="error">{error}</div>;
  return (
    <video
      key={src}
      src={src}
      controls
      autoPlay
      playsInline
      onError={() => void fallback()}
    />
  );
}
export default function Viewer({ items, index, onIndexChange, onClose }: Props) {
  const item = items[index];
  const touchStart = useRef<{ x: number; y: number } | null>(null);
  const [downloadNote, setDownloadNote] = useState("");

  const downloadCurrent = async () => {
    if (!item?.remotePath) return;
    setDownloadNote("Скачивание…");
    try {
      const saved = await api.downloadFile(
        item.remotePath,
        item.relativeDir,
        item.downloadConflict ?? "skip",
      );
      setDownloadNote(`Сохранено: ${saved}`);
    } catch (cause) {
      setDownloadNote(String(cause));
    }
  };

  const go = (delta: number) => {
    const next = Math.min(Math.max(index + delta, 0), items.length - 1);
    if (next !== index) onIndexChange(next);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
      else if (event.key === "ArrowLeft") go(-1);
      else if (event.key === "ArrowRight") go(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!item) return null;

  return (
    <div className="viewer" onClick={onClose}>
      <div
        className="viewer-inner"
        onClick={(event) => event.stopPropagation()}
        onTouchStart={(event) => {
          const touch = event.touches[0];
          touchStart.current = touch
            ? { x: touch.clientX, y: touch.clientY }
            : null;
        }}
        onTouchEnd={(event) => {
          const start = touchStart.current;
          const touch = event.changedTouches[0];
          touchStart.current = null;
          if (!start || !touch) return;
          const dx = touch.clientX - start.x;
          const dy = touch.clientY - start.y;
          if (Math.abs(dx) > 60 && Math.abs(dx) > Math.abs(dy)) {
            go(dx < 0 ? 1 : -1);
          }
        }}
      >
        <div className="viewer-head">
          <button
            className="btn"
            onClick={() => go(-1)}
            disabled={index <= 0}
            title="Предыдущее"
          >
            ‹
          </button>
          <strong>{item.name}</strong>
          <span className="chip">
            {index + 1} / {items.length}
          </span>
          <span className={`chip ${item.kind === "video" ? "warn" : "ok"}`}>
            {item.kind === "video"
              ? "video"
              : item.kind === "gif"
                ? "gif"
                  : item.kind === "pdf"
                    ? "pdf"
                    : item.kind === "text"
                      ? "text"
                      : item.kind === "document"
                        ? "document"
                        : item.kind === "archive"
                          ? "archive"
                          : "image"}
          </span>
          <span className="spacer" />
          <button
            className="btn"
            onClick={() => go(1)}
            disabled={index >= items.length - 1}
            title="Следующее"
          >
            ›
          </button>
          {item.remotePath && (
            <button className="btn" onClick={() => void downloadCurrent()}>
              Скачать
            </button>
          )}
          <button className="btn" onClick={onClose}>
            Закрыть
          </button>
        </div>
        {downloadNote && <div className="hint">{downloadNote}</div>}
        <div className="viewer-stage">
          {item.kind === "video" ? (
            <VideoViewer item={item} />
          ) : item.kind === "text" && item.remotePath ? (
            <TextViewer path={item.remotePath} />
          ) : item.kind === "document" && item.remotePath ? (
            <DocumentViewer path={item.remotePath} />
          ) : item.kind === "pdf" && item.remotePath ? (
            <PdfViewer path={item.remotePath} />
          ) : (
            <ImageViewer item={item} />
          )}
        </div>
      </div>
    </div>
  );
}
