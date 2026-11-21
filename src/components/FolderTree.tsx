import { useEffect, useState } from "react";
import { api, kindOfName } from "../api";
import type { FileEntry } from "../types";

interface Props {
  currentPath: string;
  refreshToken: number;
  showFiles: boolean;
  onOpen: (path: string) => void;
  onOpenFile: (entry: FileEntry) => void;
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
}: {
  entry: FileEntry;
  depth: number;
  currentPath: string;
  refreshToken: number;
  showFiles: boolean;
  onOpen: (path: string) => void;
  onOpenFile: (entry: FileEntry) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [children, setChildren] = useState<FileEntry[]>([]);

  const toggle = async () => {
    if (!entry.isDir) return;
    if (expanded) {
      setExpanded(false);
      return;
    }
    setExpanded(true);
    if (children.length > 0) return;
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

  useEffect(() => {
    setChildren([]);
    setExpanded(false);
  }, [showFiles, refreshToken]);

  return (
    <div>
      <div
        className={`tree-node ${currentPath === entry.path ? "active" : ""}`}
        style={{ paddingLeft: 8 + depth * 14 }}
      >
        {entry.isDir ? (
          <button className="tree-caret" onClick={() => void toggle()}>
            {expanded ? "▾" : "▸"}
          </button>
        ) : (
          <span className="tree-caret-spacer" />
        )}
        <button
          className={`tree-label ${entry.isDir ? "tree-folder-label" : "tree-file-label"}`}
          onClick={() => {
            if (entry.isDir) {
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
            key={`${refreshToken}-${child.path}`}
            entry={child}
            depth={depth + 1}
            currentPath={currentPath}
            refreshToken={refreshToken}
            showFiles={showFiles}
            onOpen={onOpen}
            onOpenFile={onOpenFile}
          />
        ))}
    </div>
  );
}

export default function FolderTree({
  currentPath,
  refreshToken,
  showFiles,
  onOpen,
  onOpenFile,
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
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshToken, showFiles]);

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
            key={`${refreshToken}-${entry.path}`}
            entry={entry}
            depth={0}
            currentPath={currentPath}
            refreshToken={refreshToken}
            showFiles={showFiles}
            onOpen={onOpen}
            onOpenFile={onOpenFile}
          />
        ))}
      </div>
    </aside>
  );
}
