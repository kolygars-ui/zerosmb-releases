import { useCallback, useEffect, useState } from "react";
import { exit, onBackButtonPress } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { api, formatBytes, kindOfName, mediaUrl } from "./api";
import type {
  ExternalEditSession,
  FileEntry,
  PublicConfig,
  TransferItem,
  TransferSnapshot,
  ViewerItem,
} from "./types";
import SetupWizard from "./components/SetupWizard";
import FilesView from "./components/FilesView";
import FolderTree from "./components/FolderTree";
import GalleryView from "./components/GalleryView";
import SyncView from "./components/SyncView";
import SettingsView from "./components/SettingsView";
import Viewer from "./components/Viewer";
import ArchiveViewer from "./components/ArchiveViewer";
import CloudMark from "./components/CloudMark";
import { applyTheme } from "./theme";

type Tab = "files" | "gallery" | "sync" | "settings";

const titles: Record<Tab, string> = {
  files: "Файлы на NAS",
  gallery: "Галерея",
  sync: "Синхронизация",
  settings: "Настройки",
};

const icons: Record<Tab, string> = {
  files: "F",
  gallery: "G",
  sync: "S",
  settings: "N",
};

const shortTitles: Record<Tab, string> = {
  files: "Файлы",
  gallery: "Галерея",
  sync: "Синхр.",
  settings: "Настройки",
};

export default function App() {
  const [config, setConfig] = useState<PublicConfig | null>(null);
  const [error, setError] = useState("");
  const [tab, setTab] = useState<Tab>("files");
  const [viewer, setViewer] = useState<{ items: ViewerItem[]; index: number } | null>(
    null,
  );
  const [treeArchive, setTreeArchive] = useState<{
    path: string;
    name: string;
  } | null>(null);
  const [refreshToken] = useState(0);
  const [filesPath, setFilesPath] = useState("");
  const [filesBackToken, setFilesBackToken] = useState(0);
  const [galleryBackToken, setGalleryBackToken] = useState(0);
  const [galleryCanBack, setGalleryCanBack] = useState(false);
  const [filesOpenRequest, setFilesOpenRequest] = useState<{
    path: string;
    token: number;
  } | null>(null);
  const [navCollapsed, setNavCollapsed] = useState(() => {
    try {
      return localStorage.getItem("zerosmb.navCollapsed") === "1";
    } catch {
      return false;
    }
  });
  const [treeVisible, setTreeVisible] = useState(true);
  const [transfers, setTransfers] = useState<TransferItem[]>([]);
  const [transferMenuOpen, setTransferMenuOpen] = useState(false);
  const [externalEdit, setExternalEdit] = useState<ExternalEditSession | null>(null);
  const [externalChanged, setExternalChanged] = useState(false);
  const [externalSaveBusy, setExternalSaveBusy] = useState(false);
  const [externalSaveMessage, setExternalSaveMessage] = useState("");

  const refresh = useCallback(async () => {
    try {
      setConfig(await api.getConfig());
      setError("");
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (config && !config.galleryEnabled && tab === "gallery") {
      setTab("files");
    }
  }, [config, tab]);

  useEffect(() => {
    if (!config) return;
    document.documentElement.style.setProperty(
      "--file-name-scale",
      String(config.fileNameScale / 100),
    );
    document.documentElement.style.setProperty(
      "--tree-font-scale",
      String(config.treeFontScale / 100),
    );
    applyTheme(config.themeColor);
  }, [config]);

  useEffect(() => {
    if (!config || config.android) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void api.transferSnapshot().then((snapshot) => {
      if (!disposed) setTransfers(snapshot.items);
    });
    void listen<TransferSnapshot>("transfer-update", (event) => {
      setTransfers(event.payload.items);
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [config?.android]);

  useEffect(() => {
    if (config?.android && config.setupComplete) {
      void api.requestNotificationPermission().catch(() => undefined);
    }
  }, [config?.android, config?.setupComplete]);

  const openViewer = useCallback(
    (item: ViewerItem, items?: ViewerItem[], index?: number) => {
      setViewer({ items: items && items.length ? items : [item], index: index ?? 0 });
    },
    [],
  );

  const openTreeFile = useCallback(
    (entry: FileEntry) => {
      const kind = kindOfName(entry.name);
      if (kind === "archive") {
        setTreeArchive({ path: entry.path, name: entry.name });
        return;
      }
      if (kind === "file") return;
      if (kind === "document") {
        if (!(config?.externalDocuments ?? true)) {
          openViewer({ remotePath: entry.path, name: entry.name, kind });
          return;
        }
        void api
          .openRemoteFile(entry.path)
          .then((file) => {
            if (config?.externalSaveEnabled) {
              setExternalEdit({
                remotePath: entry.path,
                localPath: file.localPath,
                name: entry.name,
                signature: file.signature,
              });
              setExternalChanged(false);
            }
          })
          .catch((cause) => setError(String(cause)));
        return;
      }
      if (kind === "text" || kind === "pdf") {
        openViewer({ remotePath: entry.path, name: entry.name, kind });
        return;
      }
      openViewer({
        url: mediaUrl(`smb/${entry.path}`),
        remotePath: entry.path,
        name: entry.name,
        kind,
      });
    },
    [openViewer, config?.externalDocuments, config?.externalSaveEnabled],
  );

  useEffect(() => {
    if (!config?.android) return;
    let listener: { unregister: () => Promise<void> } | null = null;
    let disposed = false;
    void onBackButtonPress(({ canGoBack }) => {
      if (viewer) {
        setViewer(null);
        return;
      }
      if (tab === "files" && filesPath) {
        setFilesBackToken((value) => value + 1);
        return;
      }
      if (tab === "gallery" && galleryCanBack) {
        setGalleryBackToken((value) => value + 1);
        return;
      }
      if (tab !== "files") {
        setTab("files");
        return;
      }
      if (canGoBack) {
        window.history.back();
        return;
      }
      void exit(0).catch(() => undefined);
    }).then((registered) => {
      if (disposed) {
        void registered.unregister();
      } else {
        listener = registered;
      }
    });
    return () => {
      disposed = true;
      void listener?.unregister();
    };
  }, [config?.android, tab, viewer, filesPath, galleryCanBack]);

  useEffect(() => {
    const onMouseBack = (event: MouseEvent) => {
      if (event.button !== 3) return;
      event.preventDefault();
      if (viewer) {
        setViewer(null);
        return;
      }
      if (tab === "files" && filesPath) {
        setFilesBackToken((value) => value + 1);
        return;
      }
      if (tab === "gallery" && galleryCanBack) {
        setGalleryBackToken((value) => value + 1);
        return;
      }
      if (tab !== "files") setTab("files");
    };
    window.addEventListener("mousedown", onMouseBack);
    return () => window.removeEventListener("mousedown", onMouseBack);
  }, [viewer, tab, filesPath, galleryCanBack]);

  const toggleNav = () => {
    setNavCollapsed((value) => {
      const next = !value;
      try {
        localStorage.setItem("zerosmb.navCollapsed", next ? "1" : "0");
      } catch {
        // storage may be unavailable
      }
      return next;
    });
  };

  useEffect(() => {
    if (!externalEdit || !config?.externalSaveEnabled) return;
    let disposed = false;
    let busy = false;
    let done = false;
    const check = async () => {
      if (disposed || busy || done || document.visibilityState === "hidden") return;
      busy = true;
      try {
        const signature = await api.externalFileSignature(externalEdit.localPath);
        if (disposed) return;
        if (signature === externalEdit.signature) {
          setExternalChanged(false);
          return;
        }
        if (config.externalSaveMode !== "auto") {
          setExternalChanged(true);
          return;
        }
        try {
          await api.saveExternalFile(
            externalEdit.remotePath,
            externalEdit.localPath,
          );
          done = true;
          if (!disposed) {
            setExternalEdit(null);
            setExternalChanged(false);
            setExternalSaveMessage("");
          }
        } catch (cause) {
          if (!disposed) {
            setExternalChanged(true);
            setExternalSaveMessage(String(cause));
          }
        }
      } catch (cause) {
        if (!disposed && externalChanged) setExternalSaveMessage(String(cause));
      } finally {
        busy = false;
      }
    };
    window.addEventListener("focus", check);
    document.addEventListener("visibilitychange", check);
    return () => {
      disposed = true;
      window.removeEventListener("focus", check);
      document.removeEventListener("visibilitychange", check);
    };
  }, [
    externalEdit,
    externalChanged,
    config?.externalSaveEnabled,
    config?.externalSaveMode,
  ]);

  if (!config) {
    return (
      <div className="setup-wrap">
        <div className="card">
          {error ? (
            <>
              <div className="error">{error}</div>
              <div className="row end" style={{ marginTop: 12 }}>
                <button className="btn" onClick={() => void refresh()}>
                  Повторить
                </button>
              </div>
            </>
          ) : (
            <div className="empty">Запуск ZeroSMB…</div>
          )}
        </div>
      </div>
    );
  }

  if (!config.setupComplete) {
    return <SetupWizard onDone={setConfig} />;
  }

  const activeTransfers = transfers.filter(
    (item) => item.status === "queued" || item.status === "active",
  );
  const completedTransfers = transfers.filter(
    (item) => item.status === "done" || item.status === "error",
  );
  const currentTransfer =
    activeTransfers.find((item) => item.status === "active") ?? activeTransfers[0];
  const percentOf = (item?: TransferItem) =>
    item && item.total > 0
      ? Math.min(100, Math.round((item.transferred / item.total) * 100))
      : 0;
  const currentPercent = percentOf(currentTransfer);
  const aggregateTotal = activeTransfers.reduce(
    (sum, item) => sum + Math.max(item.total, 0),
    0,
  );
  const aggregateTransferred = activeTransfers.reduce(
    (sum, item) => sum + Math.min(item.transferred, item.total || item.transferred),
    0,
  );
  const aggregatePercent =
    aggregateTotal > 0
      ? Math.min(100, Math.round((aggregateTransferred / aggregateTotal) * 100))
      : 0;
  const showTree = tab === "files" && treeVisible;

  return (
    <div className={`app-shell${navCollapsed ? " collapsed" : ""}${showTree ? "" : " no-tree"}`}>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <CloudMark />
          </div>
          <div className="brand-text">
            <h1>ZeroSMB</h1>
            <small>ZeroTier · SMB · NAS</small>
          </div>
          <button
            className="sidebar-toggle"
            onClick={toggleNav}
            title={navCollapsed ? "Развернуть меню" : "Свернуть меню"}
          >
            {navCollapsed ? "»" : "«"}
          </button>
        </div>
        <nav className="nav">
          {(Object.keys(titles) as Tab[])
            .filter((key) => key !== "gallery" || config.galleryEnabled)
            .map((key) => (
              <button
                key={key}
                className={tab === key ? "active" : ""}
                onClick={() => setTab(key)}
                title={titles[key]}
              >
                <span className="nav-icon">{icons[key]}</span>
                <span className="nav-label">{shortTitles[key]}</span>
              </button>
            ))}
        </nav>
        {!config.android && config.platform !== "linux" && !config.systemInstalled && (
          <button
            className="integration-banner"
            onClick={() => setTab("settings")}
            title="Настроить системную интеграцию"
          >
            Для интеграции с системой установите приложение
          </button>
        )}
        {!config.android && !navCollapsed && (
          <button
            className="transfer-panel"
            onClick={() => setTransferMenuOpen(true)}
            title="Открыть очередь передач"
          >
            <span className="transfer-panel-head">
              <span>Загрузки</span>
              <span>
                {activeTransfers.length > 0
                  ? `${currentPercent}%`
                  : completedTransfers.length > 0
                    ? `готово: ${completedTransfers.length}`
                    : "нет"}
              </span>
            </span>
            {currentTransfer ? (
              <>
                <span className="transfer-current">
                  {currentTransfer.kind === "upload" ? "↑" : "↓"}{" "}
                  {currentTransfer.name}
                </span>
                <span className="progress">
                  <span style={{ width: `${currentPercent}%` }} />
                </span>
              </>
            ) : (
              <span className="transfer-empty">Нет активных передач</span>
            )}
            {activeTransfers.length > 0 && (
              <span className="transfer-all">
                <span>
                  Все передачи <span>{aggregatePercent}%</span>
                </span>
                <span className="progress">
                  <span style={{ width: `${aggregatePercent}%` }} />
                </span>
              </span>
            )}
          </button>
        )}
        <div className="sidebar-footer">
          <div className="mono">
            {config.smb.host}/{config.smb.share}
          </div>
          <div>{config.platform === "android" ? "Android" : config.platform}</div>
          {config.zerotierNetworkId && (
            <div className="mono">{config.zerotierNetworkId}</div>
          )}
        </div>
      </aside>

      {showTree && (
        <FolderTree
          currentPath={filesPath}
          refreshToken={refreshToken}
          showFiles={config.treeShowFiles}
          onOpen={(path) => {
            setTab("files");
            setFilesOpenRequest({ path, token: Date.now() });
          }}
          onOpenFile={openTreeFile}
          onClose={() => setTreeVisible(false)}
        />
      )}

      <main className="main">
        <header className="topbar">
          <h2>{titles[tab]}</h2>
          <span className="spacer" />
          {tab === "files" && !config.android && (
            <button
              className="btn ghost"
              onClick={() => setTreeVisible((value) => !value)}
            >
              {treeVisible ? "Скрыть дерево" : "Показать дерево"}
            </button>
          )}
        </header>
        <div className="content">
          {tab === "files" && (
            <FilesView
              onOpenViewer={openViewer}
              refreshToken={refreshToken}
              backToken={filesBackToken}
              openPath={filesOpenRequest?.path}
              openPathToken={filesOpenRequest?.token}
              android={config.android}
              viewMode={config.fileViewMode}
              onPathChange={setFilesPath}
              onExternalEdit={(session) => {
                if (config?.externalSaveEnabled) {
                  setExternalEdit(session);
                  setExternalChanged(false);
                  setExternalSaveMessage("");
                }
              }}
              externalDocuments={config.externalDocuments}
            />
          )}
          {tab === "gallery" && (
            <GalleryView
              android={config.android}
              remoteGalleryPath={config.remoteGalleryPath}
              suggestedGalleryPath={config.suggestedGalleryPath}
              galleryColumns={config.galleryColumns}
              backToken={galleryBackToken}
              onPathChange={(_path, canGoBack) => setGalleryCanBack(canGoBack)}
              onOpenViewer={openViewer}
              refreshToken={refreshToken}
            />
          )}
          {tab === "sync" && (
            <SyncView
              android={config.android}
              config={config}
              onConfigChange={setConfig}
            />
          )}
          {tab === "settings" && (
            <SettingsView
              android={config.android}
              config={config}
              onConfigChange={setConfig}
            />
          )}
        </div>
      </main>

      {transferMenuOpen && !config.android && (
        <div
          className="confirm-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setTransferMenuOpen(false);
          }}
        >
          <div className="confirm-card transfer-card" role="dialog" aria-modal="true">
            <div className="row">
              <h3 style={{ margin: 0 }}>Загрузки</h3>
              <span className="spacer" />
              <button className="btn ghost" onClick={() => setTransferMenuOpen(false)}>
                Закрыть
              </button>
            </div>
            <div className="transfer-section-title">Активные</div>
            {activeTransfers.length === 0 && (
              <div className="hint">Нет активных передач</div>
            )}
            {activeTransfers.map((item) => {
              const percent = percentOf(item);
              return (
                <div className="transfer-row" key={item.id}>
                  <div className="row">
                    <span className="transfer-kind">
                      {item.kind === "upload" ? "↑" : "↓"}
                    </span>
                    <span className="transfer-name">{item.name}</span>
                    <span className="spacer" />
                    <span className="transfer-percent">{percent}%</span>
                  </div>
                  <div className="progress">
                    <div style={{ width: `${percent}%` }} />
                  </div>
                  <div className="hint">
                    {formatBytes(item.transferred)} из {formatBytes(item.total)}
                  </div>
                </div>
              );
            })}
            <div className="transfer-section-title">Завершённые</div>
            {completedTransfers.length === 0 && (
              <div className="hint">Завершённых передач пока нет</div>
            )}
            {completedTransfers.map((item) => (
              <div className="transfer-row done" key={item.id}>
                <div className="row">
                  <span className="transfer-kind">
                    {item.status === "error" ? "!" : item.kind === "upload" ? "↑" : "↓"}
                  </span>
                  <span className="transfer-name">{item.name}</span>
                  <span className="spacer" />
                  <span className="transfer-percent">
                    {item.status === "error" ? "ошибка" : "100%"}
                  </span>
                </div>
                <div className="hint">
                  {item.status === "error"
                    ? item.error
                    : `${formatBytes(item.total)} · готово`}
                </div>
              </div>
            ))}
            {completedTransfers.length > 0 && (
              <div className="row end" style={{ marginTop: 12 }}>
                <button
                  className="btn ghost"
                  onClick={async () => {
                    const snapshot = await api.transferClearCompleted();
                    setTransfers(snapshot.items);
                  }}
                >
                  Очистить завершённые
                </button>
              </div>
            )}
          </div>
        </div>
      )}

      {externalEdit && externalChanged && (
        <div className="external-save-bar">
          <div>
            <strong>{externalEdit.name}</strong>
            <div className="hint">
              {config.externalSaveMode === "auto"
                ? "Изменения сохранятся автоматически при возврате в ZeroSMB."
                : "Сохраните изменения в приложении, затем нажмите кнопку ниже."}
            </div>
          </div>
          {externalSaveMessage && (
            <div className="hint">{externalSaveMessage}</div>
          )}
          <div className="row">
            <button
              className="btn primary"
              disabled={externalSaveBusy}
              onClick={async () => {
                setExternalSaveBusy(true);
                setExternalSaveMessage("");
                try {
                  await api.saveExternalFile(
                    externalEdit.remotePath,
                    externalEdit.localPath,
                  );
                  setExternalEdit(null);
                  setExternalChanged(false);
                  setExternalSaveMessage("");
                } catch (cause) {
                  setExternalSaveMessage(String(cause));
                } finally {
                  setExternalSaveBusy(false);
                }
              }}
            >
              {externalSaveBusy ? "Сохранение…" : "Сохранить на NAS"}
            </button>
            <button
              className="btn ghost"
              onClick={() => {
                setExternalEdit(null);
                setExternalChanged(false);
                setExternalSaveMessage("");
              }}
            >
              Закрыть
            </button>
          </div>
        </div>
      )}

      <ArchiveViewer
        open={treeArchive !== null}
        path={treeArchive?.path ?? ""}
        name={treeArchive?.name ?? ""}
        externalDocuments={config.externalDocuments}
        onClose={() => setTreeArchive(null)}
      />

      {viewer && (
        <Viewer
          items={viewer.items}
          index={viewer.index}
          onIndexChange={(index) => setViewer({ ...viewer, index })}
          onClose={() => setViewer(null)}
        />
      )}
    </div>
  );
}
