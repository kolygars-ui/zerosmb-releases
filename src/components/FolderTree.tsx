import { useEffect, useRef, useState } from "react";
import { api, kindOfName } from "../api";
import type { FileEntry } from "../types";

interface Props {
  currentPath: string;
  refreshToken: number;
  share: string;
  showFiles: boolean;
  onOpen: (path: string) => void;
  onOpenFile: (entry: FileEntry) => void;
  onSelectShare: (share: string) => void;
  onClearShare: () => void;
  onClose: () => void;
}

function badgeFor(entry: FileEntry): string {
  if (entry.isDir) return "DIR";
  const kind = kindOfName(entry.name);
  if (kind === "video") return "VID";
  if (kind === "gif") return "GIF";
  if (kind === "image") return "IMG";
  if (kind === "pdf") return "PDF";
  if (kind === "text") return "TXT";
  return "FILE";
}

function sortEntries(entries: FileEntry[]): FileEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name, "ru");
  });
}

function TreeNode({
  entry,
  depth,
  currentPath,
  refreshToken,
  showFiles,
  onOpen,
  onOpenFile,
  onSelectShare,
}: {
  entry: FileEntry;
  depth: number;
  currentPath: string;
  refreshToken: number;
  showFiles: boolean;
  onOpen: (path: string) => void;
  onOpenFile: (entry: FileEntry) => void;
  onSelectShare: (share: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [children, setChildren] = useState<FileEntry[]>([]);
  const nodeRef = useRef<HTMLDivElement | null>(null);

  const loadChildren = async (force = false) => {
    if (!entry.isDir) return;
    if (!force && children.length > 0) return;
    setLoading(true);
    try {
      const list = await api.smbList(entry.path);
      setChildren(sortEntries(showFiles ? list : list.filter((item) => item.isDir)));
    } catch {
      setChildren([]);
    } finally {
      setLoading(false);
    }
  };

  const toggle = async () => {
    if (!entry.isDir) return;
    if (expanded) {
      setExpanded(false);
      return;
    }
    setExpanded(true);
    void loadChildren();
  };

  useEffect(() => {
    if (!entry.isDir) return;
    const relevant =
      currentPath === entry.path || currentPath.startsWith(`${entry.path}/`);
    if (!relevant) return;
    setExpanded(true);
    void loadChildren(true);
    if (currentPath === entry.path) {
      nodeRef.current?.scrollIntoView({ block: "nearest" });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentPath, entry.path, entry.isDir, showFiles, refreshToken]);

  return (
    <div>
      <div
        ref={nodeRef}
        className={`tree-node ${currentPath === entry.path ? "active" : ""}`}
        style={{ paddingLeft: 8 + depth * 14 }}
      >
        {entry.isDir && !entry.isShare ? (
          <button className="tree-caret" onClick={() => void toggle()}>
            {expanded ? "▾" : "▸"}
          </button>
        ) : (
          <span className="tree-caret-spacer" />
        )}
        <button
          className={`tree-label ${entry.isDir ? "tree-folder-label" : "tree-file-label"}`}
          onClick={() => {
            if (entry.isShare) {
              onSelectShare(entry.name);
            } else if (entry.isDir) {
              onOpen(entry.path);
              if (!expanded) void toggle();
            } else {
              onOpenFile(entry);
            }
          }}
          title={entry.path}
        >
          <span className={entry.isDir ? "tree-folder" : "tree-file"}>
            {badgeFor(entry)}
          </span>
          <span>{entry.name}</span>
        </button>
      </div>
      {loading && (
        <div className="tree-loading" style={{ paddingLeft: 30 + depth * 14 }}>
          …
        </div>
      )}
      {expanded &&
        children.map((child) => (
          <TreeNode
            key={child.path}
            entry={child}
            depth={depth + 1}
            currentPath={currentPath}
            refreshToken={refreshToken}
            showFiles={showFiles}
            onOpen={onOpen}
            onOpenFile={onOpenFile}
            onSelectShare={onSelectShare}
          />
        ))}
    </div>
  );
}

export default function FolderTree({
  currentPath,
  refreshToken,
  share,
  showFiles,
  onOpen,
  onOpenFile,
  onSelectShare,
  onClearShare,
  onClose,
}: Props) {
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const load = async () => {
    setLoading(true);
    setError("");
    try {
      const list = await api.smbList("");
      setEntries(sortEntries(showFiles ? list : list.filter((item) => item.isDir)));
    } catch (cause) {
      if (share) {
        setError(String(cause));
      } else {
        setEntries([]);
      }
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshToken, showFiles, share]);

  return (
    <aside className="tree-panel">
      <div className="tree-head">
        <strong>NAS</strong>
        <button
          className="btn ghost small"
          onClick={() => void load()}
          disabled={loading}
          title="Обновить дерево"
        >
          {loading ? "…" : "⟳"}
        </button>
        {share && (
          <button
            className="btn ghost small"
            onClick={onClearShare}
            title="Все шеры"
          >
            ⌂
          </button>
        )}
        <button
          className="btn ghost small"
          onClick={onClose}
          title="Скрыть дерево"
        >
          ×
        </button>
      </div>
      {error && <div className="tree-error">{error}</div>}
      <div className="tree-list">
        {entries.map((entry) => (
          <TreeNode
            key={entry.path}
            entry={entry}
            depth={0}
            currentPath={currentPath}
            refreshToken={refreshToken}
            showFiles={showFiles}
            onOpen={onOpen}
            onOpenFile={onOpenFile}
            onSelectShare={onSelectShare}
          />
        ))}
      </div>
    </aside>
  );
}
