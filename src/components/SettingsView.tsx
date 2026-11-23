import { useEffect, useState } from "react";
import { api } from "../api";
import type {
  AiStatus,
  ConnectionMode,
  PlanetInfo,
  PublicConfig,
  SmbConfig,
  UpdateInfo,
  WireGuardStatus,
  ZerotierMode,
  ZtStatus,
} from "../types";
import ConfirmDialog from "./ConfirmDialog";
import ColorPickerDialog from "./ColorPickerDialog";
import FolderPickerDialog from "./FolderPickerDialog";
import { DEFAULT_THEME_COLOR, applyTheme, normalizeHex } from "../theme";

interface Props {
  android: boolean;
  config: PublicConfig;
  onConfigChange: (config: PublicConfig) => void;
}

function Toggle({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="switch-row">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="switch-track">
        <span className="switch-thumb" />
      </span>
      <span>{label}</span>
    </label>
  );
}

export default function SettingsView({ android, config, onConfigChange }: Props) {
  const [smb, setSmb] = useState<SmbConfig>(config.smb);
  const [password, setPassword] = useState("");
  const [galleryShare, setGalleryShare] = useState(config.galleryShare);
  const [galleryPath, setGalleryPath] = useState(config.remoteGalleryPath);
  const [galleryEnabled, setGalleryEnabled] = useState(config.galleryEnabled);
  const [galleryColumns, setGalleryColumns] = useState(config.galleryColumns);
  const [downloadDir, setDownloadDir] = useState(config.downloadDir);
  const [diagnosticsPath, setDiagnosticsPath] = useState(config.diagnosticsPath);
  const [diagnosticsPickerOpen, setDiagnosticsPickerOpen] = useState(false);
  const [diagnosticsBusy, setDiagnosticsBusy] = useState(false);
  const [diagnosticsInfo, setDiagnosticsInfo] = useState("");
  const [fileNameScale, setFileNameScale] = useState(config.fileNameScale);
  const [fileViewMode, setFileViewMode] = useState(config.fileViewMode);
  const [treeShowFiles, setTreeShowFiles] = useState(config.treeShowFiles);
  const [treeFontScale, setTreeFontScale] = useState(config.treeFontScale);
  const [themeColor, setThemeColor] = useState(config.themeColor);
  const [externalDocuments, setExternalDocuments] = useState(
    config.externalDocuments,
  );
  const [externalSaveEnabled, setExternalSaveEnabled] = useState(
    config.externalSaveEnabled,
  );
  const [externalSaveMode, setExternalSaveMode] = useState<"auto" | "button">(
    config.externalSaveMode === "auto" ? "auto" : "button",
  );
  const [themePickerOpen, setThemePickerOpen] = useState(false);
  const [systemInstallDir, setSystemInstallDir] = useState(config.systemInstallDir);
  const [systemAutostart, setSystemAutostart] = useState(config.systemAutostart);
  const [systemStartMenu, setSystemStartMenu] = useState(config.systemStartMenu);
  const [systemDesktopShortcut, setSystemDesktopShortcut] = useState(
    config.systemDesktopShortcut,
  );
  const [systemTray, setSystemTray] = useState(config.systemTray);
  const [systemNotifications, setSystemNotifications] = useState(
    config.systemNotifications,
  );
  const [notifyDownloads, setNotifyDownloads] = useState(config.notifyDownloads);
  const [notifyUploads, setNotifyUploads] = useState(config.notifyUploads);
  const [notifySync, setNotifySync] = useState(config.notifySync);
  const [notifyErrors, setNotifyErrors] = useState(config.notifyErrors);
  const [downloadAutoSave, setDownloadAutoSave] = useState(config.downloadAutoSave);
  const [downloadKeepStructure, setDownloadKeepStructure] = useState(
    config.downloadKeepStructure,
  );
  const [connectionMode, setConnectionMode] = useState<ConnectionMode>(
    config.connectionMode,
  );
  const [networkId, setNetworkId] = useState(config.zerotierNetworkId);
  const [ztMode, setZtMode] = useState<ZerotierMode>(config.zerotierMode);
  const [updateRepo, setUpdateRepo] = useState(config.updateRepo);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateMessage, setUpdateMessage] = useState("");
  const [planetInfo, setPlanetInfo] = useState<PlanetInfo | null>(null);
  const [planetBusy, setPlanetBusy] = useState(false);
  const [planetMessage, setPlanetMessage] = useState("");
  const [zt, setZt] = useState<ZtStatus | null>(null);
  const [wireguard, setWireguard] = useState<WireGuardStatus | null>(null);
  const [wireguardBusy, setWireguardBusy] = useState(false);
  const [wireguardInfo, setWireguardInfo] = useState("");
  const [aiEnabled, setAiEnabled] = useState(config.aiEnabled);
  const [aiAllowDownload, setAiAllowDownload] = useState(config.aiAllowDownload);
  const [aiAllowModify, setAiAllowModify] = useState(config.aiAllowModify);
  const [aiAllowUpload, setAiAllowUpload] = useState(config.aiAllowUpload);
  const [aiAllowMkdir, setAiAllowMkdir] = useState(config.aiAllowMkdir);
  const [aiAllowDelete, setAiAllowDelete] = useState(config.aiAllowDelete);
  const [aiDenyPaths, setAiDenyPaths] = useState<string[]>(config.aiDenyPaths);
  const [aiStatus, setAiStatus] = useState<AiStatus | null>(null);
  const [aiDenyPickerOpen, setAiDenyPickerOpen] = useState(false);
  const [error, setError] = useState("");
  const [info, setInfo] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmUninstall, setConfirmUninstall] = useState(false);
  const [uninstallBusy, setUninstallBusy] = useState(false);
  const [deleteDataOnUninstall, setDeleteDataOnUninstall] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearBusy, setClearBusy] = useState(false);
  const [ztBusy, setZtBusy] = useState(false);

  useEffect(() => {
    setSmb(config.smb);
    setGalleryShare(config.galleryShare);
    setGalleryPath(config.remoteGalleryPath);
    setGalleryEnabled(config.galleryEnabled);
    setGalleryColumns(config.galleryColumns);
    setDownloadDir(config.downloadDir);
    setDiagnosticsPath(config.diagnosticsPath);
    setFileNameScale(config.fileNameScale);
    setFileViewMode(config.fileViewMode);
    setTreeShowFiles(config.treeShowFiles);
    setTreeFontScale(config.treeFontScale);
    setThemeColor(config.themeColor);
    setExternalDocuments(config.externalDocuments);
    setExternalSaveEnabled(config.externalSaveEnabled);
    setExternalSaveMode(config.externalSaveMode === "auto" ? "auto" : "button");
    setSystemInstallDir(config.systemInstallDir);
    setSystemAutostart(config.systemAutostart);
    setSystemStartMenu(config.systemStartMenu);
    setSystemDesktopShortcut(config.systemDesktopShortcut);
    setSystemTray(config.systemTray);
    setSystemNotifications(config.systemNotifications);
    setNotifyDownloads(config.notifyDownloads);
    setNotifyUploads(config.notifyUploads);
    setNotifySync(config.notifySync);
    setNotifyErrors(config.notifyErrors);
    setDownloadAutoSave(config.downloadAutoSave);
    setDownloadKeepStructure(config.downloadKeepStructure);
    setConnectionMode(config.connectionMode);
    setNetworkId(config.zerotierNetworkId);
    setZtMode(config.zerotierMode);
    setUpdateRepo(config.updateRepo);
    setAiEnabled(config.aiEnabled);
    setAiAllowDownload(config.aiAllowDownload);
    setAiAllowModify(config.aiAllowModify);
    setAiAllowUpload(config.aiAllowUpload);
    setAiAllowMkdir(config.aiAllowMkdir);
    setAiAllowDelete(config.aiAllowDelete);
    setAiDenyPaths(config.aiDenyPaths);
  }, [config]);

  useEffect(() => {
    void api.wireguardStatus().then(setWireguard).catch(() => undefined);
    void api.aiStatus().then(setAiStatus).catch(() => undefined);
  }, []);

  useEffect(() => {
    void api.ztStatus(ztMode).then(setZt).catch(() => undefined);
    void api.getPlanet(ztMode).then(setPlanetInfo).catch(() => undefined);
  }, [ztMode]);

  useEffect(() => {
    if (android && ztMode === "system") setNetworkId("");
  }, [android, ztMode]);

  const updateSmb = <K extends keyof SmbConfig>(key: K, value: SmbConfig[K]) => {
    setSmb((current) => ({ ...current, [key]: value }));
  };

  const save = async () => {
    setBusy(true);
    setError("");
    setInfo("");
    try {
      const next = await api.saveSettings({
        smb,
        password: password.trim() || undefined,
        galleryShare: galleryShare.trim(),
        remoteGalleryPath: galleryPath.trim(),
        galleryEnabled,
        galleryColumns,
        downloadDir,
        diagnosticsPath,
        fileNameScale,
        fileViewMode,
        treeShowFiles,
        treeFontScale,
        themeColor,
        externalDocuments,
        externalSaveEnabled,
        externalSaveMode,
        systemInstallDir,
        systemAutostart,
        systemStartMenu,
        systemDesktopShortcut,
        systemTray,
        systemNotifications,
        notifyDownloads,
        notifyUploads,
        notifySync,
        notifyErrors,
        downloadAutoSave,
        downloadKeepStructure,
        zerotierNetworkId: networkId.trim(),
        connectionMode,
        zerotierMode: ztMode,
        aiEnabled,
        aiAllowDownload,
        aiAllowModify,
        aiAllowUpload,
        aiAllowMkdir,
        aiAllowDelete,
        aiDenyPaths,
        updateRepo: updateRepo.trim(),
      });
      onConfigChange(next);
      setAiStatus(await api.aiStatus());
      setPassword("");
      setInfo("Настройки сохранены");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const sendErrorLog = async () => {
    setDiagnosticsBusy(true);
    setError("");
    setDiagnosticsInfo("");
    try {
      if (!diagnosticsPath.trim()) {
        throw new Error("Укажите папку на NAS для файла ошибок");
      }
      const message = await api.sendErrorLog(diagnosticsPath.trim());
      setDiagnosticsInfo(message);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setDiagnosticsBusy(false);
    }
  };

  const applySystem = async () => {
    await save();
    setBusy(true);
    setError("");
    setInfo("");
    try {
      const message = await api.installSystem(systemInstallDir.trim());
      onConfigChange(await api.getConfig());
      setInfo(message);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const uninstall = async () => {
    setUninstallBusy(true);
    setError("");
    setInfo("");
    try {
      const message = await api.uninstallSystem(deleteDataOnUninstall);
      setInfo(message);
      onConfigChange(await api.getConfig());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setUninstallBusy(false);
    }
  };

  const clearData = async () => {
    setClearBusy(true);
    setError("");
    setInfo("");
    try {
      const message = await api.clearAppData();
      setInfo(message);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setClearBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    setError("");
    setInfo("");
    try {
      if (password.trim()) {
        await api.testConnection(smb, password, networkId.trim(), connectionMode, ztMode);
      } else {
        await api.testConnectionSaved(smb, networkId.trim(), connectionMode, ztMode);
      }
      setInfo("Соединение установлено");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const refreshZt = async () => {
    try {
      setZt(await api.ztStatus(ztMode));
      setPlanetInfo(await api.getPlanet(ztMode));
    } catch (cause) {
      setError(String(cause));
    }
  };

  const join = async () => {
    setZtBusy(true);
    setError("");
    setInfo("");
    try {
      const status = await api.ztJoin(networkId.trim(), ztMode);
      setZt(status);
      setInfo(status.message);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setZtBusy(false);
    }
  };

  const leave = async (id = networkId) => {
    setZtBusy(true);
    setError("");
    try {
      const status = await api.ztLeave(id.trim(), ztMode);
      setZt(status);
      setInfo("Сеть отключена");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setZtBusy(false);
    }
  };

  const refreshWireGuard = async () => {
    setWireguard(await api.wireguardStatus());
  };

  const copyText = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setInfo("Скопировано в буфер обмена");
    } catch (cause) {
      setError(String(cause));
    }
  };

  const pickWireGuard = async () => {
    setWireguardBusy(true);
    setError("");
    setWireguardInfo("");
    try {
      const path = await api.wireguardPickConfig();
      if (path) {
        onConfigChange(await api.getConfig());
        await refreshWireGuard();
        setWireguardInfo("Конфигурация выбрана");
      }
    } catch (cause) {
      setError(String(cause));
    } finally {
      setWireguardBusy(false);
    }
  };

  const connectWireGuard = async () => {
    setWireguardBusy(true);
    setError("");
    setWireguardInfo("");
    try {
      setWireguardInfo(await api.wireguardConnect());
      onConfigChange(await api.getConfig());
      await refreshWireGuard();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setWireguardBusy(false);
    }
  };

  const disconnectWireGuard = async () => {
    setWireguardBusy(true);
    setError("");
    setWireguardInfo("");
    try {
      setWireguardInfo(await api.wireguardDisconnect());
      onConfigChange(await api.getConfig());
      await refreshWireGuard();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setWireguardBusy(false);
    }
  };

  const checkUpdates = async () => {
    setUpdateBusy(true);
    setUpdateMessage("");
    setUpdateInfo(null);
    try {
      const result = await api.checkUpdate(updateRepo.trim() || undefined);
      setUpdateInfo(result);
      setUpdateMessage(result.message);
    } catch (cause) {
      setUpdateMessage(String(cause));
    } finally {
      setUpdateBusy(false);
    }
  };

  const installUpdate = async () => {
    if (!updateInfo?.assetUrl || !updateInfo.assetName) return;
    setUpdateBusy(true);
    setUpdateMessage("Скачивание обновления…");
    try {
      const message = await api.installUpdate(updateInfo.assetUrl, updateInfo.assetName);
      setUpdateMessage(message);
    } catch (cause) {
      setUpdateMessage(String(cause));
    } finally {
      setUpdateBusy(false);
    }
  };

  const selectPlanet = async () => {
    setPlanetBusy(true);
    setPlanetMessage("");
    try {
      const result = await api.pickPlanet(ztMode);
      setPlanetInfo(result);
      setPlanetMessage(
        ztMode === "system"
          ? "planet установлен в системный ZeroTier."
          : "planet выбран. Нажмите «Переподключить» в карточке ZeroTier, чтобы использовать только свой корневой сервер.",
      );
    } catch (cause) {
      setPlanetMessage(String(cause));
    } finally {
      setPlanetBusy(false);
    }
  };

  const removePlanet = async () => {
    setPlanetBusy(true);
    setPlanetMessage("");
    try {
      const result = await api.clearPlanet(ztMode);
      setPlanetInfo(result);
      setPlanetMessage(result.message);
    } catch (cause) {
      setPlanetMessage(String(cause));
    } finally {
      setPlanetBusy(false);
    }
  };
  const isLinux = config.platform === "linux";
  const planetReady =
    ztMode === "system" ? planetInfo?.systemConfigured : planetInfo?.configured;
  const planetRemovable =
    ztMode === "system"
      ? Boolean(planetInfo?.systemConfigured && planetInfo?.systemManaged)
      : Boolean(planetInfo?.configured);
  const planetSize =
    ztMode === "system" ? planetInfo?.systemSize ?? 0 : planetInfo?.size ?? 0;
  return (
    <div className="stack">
      {error && <div className="error">{error}</div>}
      {info && <div className="success">{info}</div>}
      <div className="card">
        <h3>Персонализация</h3>
        <div className="grid two">
          <div className="field">
            <label>Размер шрифта названий файлов и папок</label>
            <select
              value={fileNameScale}
              onChange={(event) => setFileNameScale(Number(event.target.value))}
            >
              {[100, 110, 120, 130, 150, 170].map((value) => (
                <option key={value} value={value}>
                  {value}%
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label>Вид файлов</label>
            <select
              value={fileViewMode}
              onChange={(event) =>
                setFileViewMode(event.target.value as "list" | "details")
              }
            >
              <option value="list">Список</option>
              <option value="details">Таблица: имя, размер, тип, дата</option>
            </select>
          </div>
          {!android && (
            <div className="field">
              <label>Размер шрифта дерева</label>
              <select
                value={treeFontScale}
                onChange={(event) => setTreeFontScale(Number(event.target.value))}
              >
                {[100, 110, 120, 130, 150, 170].map((value) => (
                  <option key={value} value={value}>
                    {value}%
                  </option>
                ))}
              </select>
            </div>
          )}
          <div className="field">
            <label>Карточек в строке галереи</label>
            <select
              value={galleryColumns}
              onChange={(event) => setGalleryColumns(Number(event.target.value))}
            >
              {[2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12].map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="field" style={{ marginTop: 12 }}>
          <label>Цвет оформления</label>
          <div className="row">
            <button
              className="theme-swatch"
              style={{ background: normalizeHex(themeColor) }}
              onClick={() => setThemePickerOpen(true)}
              title="Изменить цвет оформления"
            />
            <button className="btn" onClick={() => setThemePickerOpen(true)}>
              Изменить цвет
            </button>
            <button
              className="btn ghost"
              onClick={() => {
                setThemeColor("");
                applyTheme(DEFAULT_THEME_COLOR);
              }}
            >
              Исходный цвет
            </button>
            <span className="mono small">{normalizeHex(themeColor)}</span>
          </div>
          <span className="hint">
            Цвет фона приложения. Текст автоматически меняет контраст для светлых и
            тёмных цветов.
          </span>
        </div>
        {!android && (
          <label className="check-row" style={{ marginTop: 12 }}>
            <input
              type="checkbox"
              checked={treeShowFiles}
              onChange={(event) => setTreeShowFiles(event.target.checked)}
            />
            Показывать файлы в папках
          </label>
        )}
        <label className="check-row">
          <input
            type="checkbox"
            checked={galleryEnabled}
            onChange={(event) => setGalleryEnabled(event.target.checked)}
          />
          Показывать вкладку «Галерея»
        </label>
        <div className="row" style={{ marginTop: 12 }}>
          <button className="btn primary" onClick={() => void save()} disabled={busy}>
            {busy ? "Сохранение…" : "Сохранить"}
          </button>
        </div>
      </div>
      <div className="card">
        <div className="row">
          <div>
            <h3 style={{ marginBottom: 2 }}>Доступ для ИИ-агентов</h3>
            <span className="small muted">
              Локальный API на 127.0.0.1 для доступа к файлам NAS
            </span>
          </div>
          <span className="spacer" />
          <span
            className={`chip ${aiEnabled && aiStatus?.running ? "ok" : "warn"}`}
          >
            {aiEnabled
              ? aiStatus?.running
                ? "API работает"
                : "API остановлен"
              : "выключено"}
          </span>
        </div>
        <Toggle
          checked={aiEnabled}
          label="Разрешить доступ ИИ-агентам"
          onChange={setAiEnabled}
        />
        {aiEnabled && (
          <>
            <div className="grid two" style={{ marginTop: 4 }}>
              <Toggle
                checked={aiAllowDownload}
                label="Скачивание и чтение"
                onChange={setAiAllowDownload}
              />
              <Toggle
                checked={aiAllowModify}
                label="Изменение файлов"
                onChange={setAiAllowModify}
              />
              <Toggle
                checked={aiAllowUpload}
                label="Загрузка новых файлов"
                onChange={setAiAllowUpload}
              />
              <Toggle
                checked={aiAllowMkdir}
                label="Создание папок"
                onChange={setAiAllowMkdir}
              />
              <Toggle
                checked={aiAllowDelete}
                label="Удаление файлов и папок"
                onChange={setAiAllowDelete}
              />
            </div>
            <div className="field" style={{ marginTop: 14 }}>
              <label>Запрещённые папки</label>
              <div className="row">
                <button className="btn" onClick={() => setAiDenyPickerOpen(true)}>
                  Добавить папку
                </button>
              </div>
              {aiDenyPaths.length > 0 ? (
                <div className="deny-list">
                  {aiDenyPaths.map((path) => (
                    <div className="deny-row" key={path}>
                      <span className="mono">{path}</span>
                      <button
                        className="deny-remove"
                        onClick={() =>
                          setAiDenyPaths((current) =>
                            current.filter((item) => item !== path),
                          )
                        }
                        title="Убрать из запрета"
                      >
                        ×
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <span className="hint">
                  Запрещённых папок нет. Запрет действует на папку и всё её содержимое.
                </span>
              )}
            </div>
            <div className="field" style={{ marginTop: 12 }}>
              <label>Endpoint</label>
              <div className="row">
                <input
                  readOnly
                  value={aiStatus?.url || "http://127.0.0.1:17845"}
                  className="mono"
                />
                <button
                  className="btn"
                  onClick={() =>
                    void copyText(aiStatus?.url || "http://127.0.0.1:17845")
                  }
                >
                  Копировать
                </button>
              </div>
            </div>
            <div className="hint" style={{ marginTop: 10 }}>
              Токен не нужен: API доступен без авторизации только с этого компьютера.
              Первый запрос к{" "}
              <span className="mono">{aiStatus?.url || "http://127.0.0.1:17845"}/</span>{" "}
              возвращает все инструкции для ИИ-агента.
            </div>
            {aiStatus?.error && (
              <div className="error" style={{ marginTop: 10 }}>
                {aiStatus.error}
              </div>
            )}
          </>
        )}
        <div className="row" style={{ marginTop: 14 }}>
          <button className="btn primary" onClick={() => void save()} disabled={busy}>
            {busy ? "Сохранение…" : "Сохранить доступ ИИ"}
          </button>
        </div>
      </div>
      {android && (
        <div className="card">
          <h3>Уведомления</h3>
          <label className="check-row">
            <input
              type="checkbox"
              checked={systemNotifications}
              onChange={(event) => setSystemNotifications(event.target.checked)}
            />
            Показывать системные уведомления
          </label>
          <div className="row" style={{ marginTop: 8 }}>
            <button
              className="btn"
              onClick={() => void api.requestNotificationPermission().catch(() => undefined)}
            >
              Разрешить уведомления
            </button>
            <button
              className="btn"
              onClick={async () => {
                try {
                  setInfo(await api.testNotification());
                } catch (cause) {
                  setError(String(cause));
                }
              }}
            >
              Проверить уведомление
            </button>
          </div>
          {systemNotifications && (
            <div className="row" style={{ marginTop: 8 }}>
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={notifyDownloads}
                  onChange={(event) => setNotifyDownloads(event.target.checked)}
                />
                Скачивание
              </label>
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={notifyUploads}
                  onChange={(event) => setNotifyUploads(event.target.checked)}
                />
                Загрузка
              </label>
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={notifySync}
                  onChange={(event) => setNotifySync(event.target.checked)}
                />
                Синхронизация
              </label>
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={notifyErrors}
                  onChange={(event) => setNotifyErrors(event.target.checked)}
                />
                Ошибки
              </label>
            </div>
          )}
        </div>
      )}
      {android && (
        <div className="card">
          <h3>Документы</h3>
          <label className="check-row">
            <input
              type="checkbox"
              checked={externalDocuments}
              onChange={(event) => setExternalDocuments(event.target.checked)}
            />
            Открывать документы во внешних приложениях
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={externalSaveEnabled}
              onChange={(event) => setExternalSaveEnabled(event.target.checked)}
            />
            Сохранять изменения обратно на NAS
          </label>
          <div className="field" style={{ maxWidth: 320 }}>
            <label>Способ сохранения</label>
            <select
              value={externalSaveMode}
              onChange={(event) =>
                setExternalSaveMode(event.target.value === "auto" ? "auto" : "button")
              }
              disabled={!externalSaveEnabled}
            >
              <option value="auto">Автоматически при возврате в ZeroSMB</option>
              <option value="button">По кнопке «Сохранить на NAS»</option>
            </select>
          </div>
          <div className="row" style={{ marginTop: 12 }}>
            <button className="btn primary" onClick={() => void save()} disabled={busy}>
              {busy ? "Сохранение…" : "Сохранить"}
            </button>
          </div>
        </div>
      )}
      {!android && (
        <div className="card">
          <h3>Системная интеграция</h3>
          {!isLinux && config.systemInstalled && (
            <span className="chip ok">Установлено в систему</span>
          )}
          <label className="check-row" style={{ marginTop: 12 }}>
            <input
              type="checkbox"
              checked={externalDocuments}
              onChange={(event) => setExternalDocuments(event.target.checked)}
            />
            Открывать документы во внешних приложениях
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={externalSaveEnabled}
              onChange={(event) => setExternalSaveEnabled(event.target.checked)}
            />
            Сохранять изменения обратно на NAS
          </label>
          <div className="field" style={{ maxWidth: 320 }}>
            <label>Способ сохранения</label>
            <select
              value={externalSaveMode}
              onChange={(event) =>
                setExternalSaveMode(event.target.value === "auto" ? "auto" : "button")
              }
              disabled={!externalSaveEnabled}
            >
              <option value="auto">Автоматически при возврате в ZeroSMB</option>
              <option value="button">По кнопке «Сохранить на NAS»</option>
            </select>
          </div>
          <span className="hint">
            Если открытие во внешнем приложении выключено, используется встроенный
            упрощённый текстовый просмотр.
          </span>
          {!isLinux && (
            <div className="field" style={{ marginTop: 12 }}>
              <label>Папка установки</label>
              <div className="row">
                <input
                  value={systemInstallDir}
                  onChange={(event) => setSystemInstallDir(event.target.value)}
                  placeholder="C:\\Users\\Имя\\AppData\\Local\\Programs\\ZeroSMB"
                  spellCheck={false}
                />
                <button
                  className="btn"
                  onClick={async () => {
                    try {
                      const picked = await api.pickLocalFolder();
                      if (picked) setSystemInstallDir(picked);
                    } catch (cause) {
                      setError(String(cause));
                    }
                  }}
                >
                  Выбрать
                </button>
              </div>
              <span className="hint">
                Приложение будет скопировано в эту папку. При смене папки установка
                переносится.
              </span>
            </div>
          )}
          <label className="check-row">
            <input
              type="checkbox"
              checked={systemAutostart}
              onChange={(event) => setSystemAutostart(event.target.checked)}
            />
            Запускать при входе в систему
          </label>
          {!isLinux && (
            <label className="check-row">
              <input
                type="checkbox"
                checked={systemStartMenu}
                onChange={(event) => setSystemStartMenu(event.target.checked)}
              />
              Добавить в меню «Пуск» / меню приложений
            </label>
          )}
          {!isLinux && (
            <label className="check-row">
              <input
                type="checkbox"
                checked={systemDesktopShortcut}
                onChange={(event) => setSystemDesktopShortcut(event.target.checked)}
              />
              Создать ярлык на рабочем столе
            </label>
          )}
          <label className="check-row">
            <input
              type="checkbox"
              checked={systemTray}
              onChange={(event) => setSystemTray(event.target.checked)}
            />
            Работать в фоне и сворачиваться в трей
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={systemNotifications}
              onChange={(event) => setSystemNotifications(event.target.checked)}
            />
            Показывать системные уведомления
          </label>
          <div className="row" style={{ marginTop: 8 }}>
            <button
              className="btn"
              onClick={async () => {
                try {
                  setInfo(await api.testNotification());
                } catch (cause) {
                  setError(String(cause));
                }
              }}
            >
              Проверить уведомление
            </button>
          </div>
          {systemNotifications && (
            <div className="row" style={{ marginTop: 8 }}>
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={notifyDownloads}
                  onChange={(event) => setNotifyDownloads(event.target.checked)}
                />
                Скачивание
              </label>
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={notifyUploads}
                  onChange={(event) => setNotifyUploads(event.target.checked)}
                />
                Загрузка
              </label>
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={notifySync}
                  onChange={(event) => setNotifySync(event.target.checked)}
                />
                Синхронизация
              </label>
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={notifyErrors}
                  onChange={(event) => setNotifyErrors(event.target.checked)}
                />
                Ошибки
              </label>
            </div>
          )}
          <div className="field" style={{ marginTop: 16 }}>
            <h3 style={{ marginBottom: 2 }}>Загрузки на ПК</h3>
            <label>Папка, куда сохранять скачанное</label>
            <div className="row">
              <input
                value={downloadDir}
                onChange={(event) => setDownloadDir(event.target.value)}
                placeholder="Например, G:\Downloads\ZeroSMB"
                spellCheck={false}
              />
              <button
                className="btn"
                onClick={async () => {
                  try {
                    const picked = await api.pickLocalFolder();
                    if (picked) setDownloadDir(picked);
                  } catch (cause) {
                    setError(String(cause));
                  }
                }}
              >
                Выбрать папку
              </button>
            </div>
            <span className="hint">
              Если указана, скачивание идёт сразу туда без диалога.
            </span>
          </div>
          <div className="row" style={{ marginTop: 10 }}>
            <label className="check-row" style={{ marginTop: 0 }}>
              <input
                type="checkbox"
                checked={downloadAutoSave}
                onChange={(event) => setDownloadAutoSave(event.target.checked)}
              />
              Сохранять без подтверждения в выбранную папку
            </label>
            <label className="check-row" style={{ marginTop: 0 }}>
              <input
                type="checkbox"
                checked={downloadKeepStructure}
                onChange={(event) =>
                  setDownloadKeepStructure(event.target.checked)
                }
              />
              Сохранять структуру папок
            </label>
          </div>
          <div className="row" style={{ marginTop: 12 }}>
            <button
              className="btn primary"
              onClick={() => void (isLinux ? save() : applySystem())}
              disabled={busy}
            >
              {busy ? "Сохранение…" : isLinux ? "Сохранить" : "Установить / применить"}
            </button>
            {!isLinux && (
              <button
                className="btn danger ghost"
                onClick={() => setConfirmUninstall(true)}
                disabled={busy || uninstallBusy}
              >
                Удалить из системы
              </button>
            )}
            <button
              className="btn ghost"
              onClick={() => setConfirmClear(true)}
              disabled={busy || clearBusy}
            >
              Очистить кэш и данные
            </button>
          </div>
        </div>
      )}
      <div className="card">
        <div className="row">
          <div>
            <h3 style={{ marginBottom: 2 }}>Подключение</h3>
            <span className="small muted">
              Как ZeroSMB добирается до NAS. Настройки SMB ниже не меняются.
            </span>
          </div>
          <span className="spacer" />
          <span
            className={`chip ${
              connectionMode === "wireguard"
                ? wireguard?.connected
                  ? "ok"
                  : "warn"
                : connectionMode === "zerotier"
                  ? ztMode === "system" && android
                    ? "ok"
                    : zt?.serviceOnline
                      ? "ok"
                      : "warn"
                  : "ok"
            }`}
          >
            {connectionMode === "wireguard"
              ? wireguard?.connected
                ? "подключён"
                : "отключён"
              : connectionMode === "zerotier"
                ? ztMode === "system" && android
                  ? "приложение"
                  : zt?.serviceOnline
                    ? "online"
                    : "offline"
                : "локально"}
          </span>
        </div>

        <div className="setup-choices" style={{ marginTop: 14 }}>
          <button
            className={`setup-choice ${connectionMode === "zerotier" ? "active" : ""}`}
            onClick={() => setConnectionMode("zerotier")}
          >
            <span className="setup-choice-title">ZeroTier</span>
            <span className="setup-choice-text">Сеть по Network ID</span>
          </button>
          <button
            className={`setup-choice ${connectionMode === "wireguard" ? "active" : ""}`}
            onClick={() => setConnectionMode("wireguard")}
          >
            <span className="setup-choice-title">WireGuard</span>
            <span className="setup-choice-text">Свой VPN по .conf</span>
          </button>
          <button
            className={`setup-choice ${connectionMode === "local" ? "active" : ""}`}
            onClick={() => setConnectionMode("local")}
          >
            <span className="setup-choice-title">Local</span>
            <span className="setup-choice-text">Напрямую по LAN/IP</span>
          </button>
        </div>

        {connectionMode === "local" && (
          <div className="hint" style={{ marginTop: 14 }}>
            Локальный режим: VPN не используется, SMB подключается напрямую по адресу
            из блока «NAS / SMB» ниже.
          </div>
        )}

        {connectionMode === "zerotier" && (
          <div style={{ marginTop: 14 }}>
            <div className="setup-choices">
              <button
                className={`setup-choice ${ztMode === "system" ? "active" : ""}`}
                onClick={() => setZtMode("system")}
              >
                <span className="setup-choice-title">
                  Системный ZeroTier {android ? "" : "(рекомендуется)"}
                </span>
                <span className="setup-choice-text">
                  {android
                    ? "Приложение ZeroTier One"
                    : "Установленный ZeroTier One, управление сетями здесь"}
                </span>
              </button>
              <button
                className={`setup-choice ${ztMode === "embedded" ? "active" : ""}`}
                onClick={() => setZtMode("embedded")}
              >
                <span className="setup-choice-title">Встроенный модуль</span>
                <span className="setup-choice-text">libzt внутри ZeroSMB</span>
              </button>
            </div>

            {ztMode === "system" && (
              <div className="hint" style={{ marginTop: 10 }}>
                {android
                  ? "На Android сети ZeroTier добавляются и удаляются в приложении ZeroTier One."
                  : "ZeroSMB управляет установленным ZeroTier One: добавление и удаление сетей, установка planet."}
              </div>
            )}
            {isLinux && ztMode === "system" && (
              <div className="hint">
                Поддерживается установка через apt (`zerotier-one`) и snap (`zerotier`).
              </div>
            )}
            {android && ztMode === "system" ? (
              <div className="row" style={{ marginTop: 12 }}>
                <button className="btn primary" onClick={() => void api.ztOpenAndroid("")}>
                  Открыть приложение ZeroTier
                </button>
              </div>
            ) : (
              <>
            <div className="row" style={{ marginTop: 10 }}>
              <button className="btn ghost" onClick={() => void api.ztOpenHelp()}>
                {android ? "Открыть приложение ZeroTier" : "Скачать ZeroTier"}
              </button>
              <button className="btn" onClick={() => void refreshZt()} disabled={ztBusy}>
                Обновить
              </button>
            </div>

            {!(android && ztMode === "system") && (
              <div
                className="card"
                style={{
                  marginTop: 12,
                  marginBottom: 12,
                  background: "rgba(6, 10, 22, 0.35)",
                }}
              >
                <div className="row">
                  <div>
                    <h3 style={{ marginBottom: 2 }}>Корневые серверы ZeroTier (planet)</h3>
                    <span className="small muted">
                      {ztMode === "system"
                        ? "Свой planet для системного ZeroTier. Windows запросит UAC, Linux — pkexec."
                        : "Свой planet для встроенного ZeroTier."}
                    </span>
                  </div>
                  <span className="spacer" />
                  <span className={`chip ${planetReady ? "ok" : "warn"}`}>
                    {planetReady ? "свой planet" : "стандартные"}
                  </span>
                </div>
                <div className="small" style={{ marginTop: 8 }}>
                  {planetReady
                    ? `Свой planet: ${planetSize} байт${
                        ztMode === "system" && planetInfo?.systemWorldId
                          ? ` · world ${planetInfo.systemWorldId}`
                          : ""
                      }`
                    : "Свой planet не установлен — используются стандартные корни"}
                </div>
                {ztMode === "system" && planetInfo?.systemLocation && (
                  <div className="hint mono" style={{ marginTop: 6 }}>
                    {planetInfo.systemLocation}
                  </div>
                )}
                {planetReady && ztMode === "system" && !planetRemovable && (
                  <div className="hint" style={{ marginTop: 6 }}>
                    planet уже изменён вручную; автоматическое удаление доступно только для
                    planet, установленного через ZeroSMB.
                  </div>
                )}
                <div className="row" style={{ marginTop: 10 }}>
                  <button
                    className="btn primary"
                    onClick={() => void selectPlanet()}
                    disabled={planetBusy}
                  >
                    {planetBusy ? "Чтение…" : "Выбрать planet…"}
                  </button>
                  {planetRemovable && (
                    <button
                      className="btn danger"
                      onClick={() => void removePlanet()}
                      disabled={planetBusy}
                    >
                      Удалить planet
                    </button>
                  )}
                  {ztMode === "embedded" && planetReady && (
                    <button
                      className="btn"
                      onClick={() => void join()}
                      disabled={ztBusy || planetBusy}
                    >
                      Переподключить ZeroTier
                    </button>
                  )}
                </div>
                {planetMessage && <div className="hint" style={{ marginTop: 8 }}>{planetMessage}</div>}
              </div>
            )}

            <div className="row" style={{ marginTop: 12 }}>
              <div className="field" style={{ flex: 1, minWidth: 220 }}>
                <label>Network ID <span className="req">*</span></label>
                <input
                  value={networkId}
                  onChange={(event) => setNetworkId(event.target.value)}
                  className="mono"
                  spellCheck={false}
                />
                <span className="hint">
                  {zt?.available
                    ? `${ztMode === "system" ? "Системный" : "Встроенный"} ZeroTier${
                        zt.version ? ` · ${zt.version}` : ""
                      }${zt.address ? ` · ${zt.address}` : ""}`
                    : zt?.message ||
                      (ztMode === "system"
                        ? "ZeroTier One не найден или служба не запущена"
                        : "Встроенный ZeroTier будет запущен при подключении")}
                </span>
              </div>
              {ztMode === "system" && android ? (
                <button
                  className="btn primary"
                  onClick={() => void api.ztOpenAndroid(networkId.trim())}
                  disabled={!networkId.trim()}
                >
                  Открыть ZeroTier
                </button>
              ) : (
                <>
                  <button
                    className="btn primary"
                    onClick={() => void join()}
                    disabled={ztBusy || !networkId.trim()}
                  >
                    {ztMode === "system" ? "Добавить сеть" : "Подключить"}
                  </button>
                  {!android && (
                    <button className="btn" onClick={() => void leave()} disabled={ztBusy}>
                      Отключить
                    </button>
                  )}
                </>
              )}
            </div>
            {zt?.networks?.length ? (
              <div style={{ marginTop: 12 }}>
                {zt.networks.map((network) => (
                  <div className="zt-network" key={network.id}>
                    <div>
                      <div className="mono">{network.id}</div>
                      <div className="small muted">
                        {network.name || "Без названия"}
                        {network.assignedAddresses.length
                          ? ` · ${network.assignedAddresses.join(", ")}`
                          : ""}
                      </div>
                    </div>
                    <div className="row">
                      <span
                        className={`chip ${
                          network.status === "OK" || network.status === "APP" ? "ok" : "warn"
                        }`}
                      >
                        {network.status === "APP" ? "в приложении" : network.status}
                      </span>
                      {!android && ztMode === "system" && (
                        <button
                          className="btn danger ghost"
                          onClick={() => void leave(network.id)}
                          disabled={ztBusy}
                        >
                          Удалить
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="hint" style={{ marginTop: 10 }}>
                {zt?.message || "Нет данных о сетях"}
              </div>
            )}
              </>
            )}
          </div>
        )}

        {connectionMode === "wireguard" && (
          <div style={{ marginTop: 14 }}>
            <div className="field">
              <label>Конфигурация WireGuard (.conf)</label>
              <div className="row">
                <input
                  value={wireguard?.configPath || ""}
                  readOnly
                  placeholder="Выберите .conf с сервером WireGuard"
                  spellCheck={false}
                />
                <button
                  className="btn"
                  onClick={() => void pickWireGuard()}
                  disabled={wireguardBusy}
                >
                  Выбрать
                </button>
              </div>
              <span className="hint">
                Файл содержит адрес WireGuard-сервера и ключи. Он хранится в приватной
                папке приложения.
              </span>
            </div>
            <div className="row" style={{ marginTop: 12 }}>
              <button
                className="btn primary"
                onClick={() => void connectWireGuard()}
                disabled={wireguardBusy || !wireguard?.configured}
              >
                {wireguard?.connected ? "Переподключить" : "Подключить"}
              </button>
              <button
                className="btn"
                onClick={() => void disconnectWireGuard()}
                disabled={wireguardBusy || !wireguard?.connected}
              >
                Отключить
              </button>
              {!android && (
                <button className="btn ghost" onClick={() => void api.wireguardOpenHelp()}>
                  Помощь
                </button>
              )}
            </div>
            {wireguardInfo && (
              <div className="success" style={{ marginTop: 10 }}>
                {wireguardInfo}
              </div>
            )}
          </div>
        )}

        <div className="row" style={{ marginTop: 14 }}>
          <button className="btn primary" onClick={() => void save()} disabled={busy}>
            {busy ? "Сохранение…" : "Сохранить подключение"}
          </button>
        </div>
      </div>

      <div className="card">
        <h3>NAS / SMB</h3>
        {connectionMode === "local" && (
          <div className="hint">
            Локальный режим: {smb.host || "укажите LAN-IP"}:{smb.port}. Устройство должно
            быть в той же Wi-Fi/LAN-сети, что и NAS.
          </div>
        )}
        {connectionMode === "zerotier" && (
          <div className="hint">
            ZeroTier: укажите адрес NAS внутри вашей сети ZeroTier.
          </div>
        )}
        {connectionMode === "wireguard" && (
          <div className="hint">
            WireGuard: укажите адрес NAS внутри вашей WireGuard-сети. Для внутренних имён
            используйте IP.
          </div>
        )}
        <div className="grid two">
          <div className="field">
            <label>Host / IP <span className="req">*</span></label>
            <input value={smb.host} onChange={(e) => updateSmb("host", e.target.value)} />
          </div>
          <div className="field">
            <label>Share</label>
            <input
              value={smb.share}
              onChange={(e) => updateSmb("share", e.target.value)}
              placeholder="Оставьте пустым для всех шеров"
            />
            <span className="hint">Пусто — показать все доступные шеры.</span>
          </div>
          <div className="field">
            <label>Порт <span className="req">*</span></label>
            <input
              type="number"
              value={smb.port}
              onChange={(e) => updateSmb("port", Number(e.target.value) || 445)}
            />
          </div>
          <div className="field">
            <label>Домен</label>
            <input value={smb.domain} onChange={(e) => updateSmb("domain", e.target.value)} />
          </div>
          <div className="field">
            <label>Пользователь <span className="req">*</span></label>
            <input
              value={smb.username}
              onChange={(e) => updateSmb("username", e.target.value)}
              autoCapitalize="none"
            />
          </div>
          <div className="field">
            <label>
              Новый пароль {config.hasPassword ? "(оставьте пустым, чтобы не менять)" : ""}
            </label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="new-password"
            />
          </div>
          <div className="field">
            <label>Базовая папка</label>
            <input
              value={smb.basePath}
              onChange={(e) => updateSmb("basePath", e.target.value)}
            />
          </div>
          <div className="field">
            <label>Share для галереи</label>
            <input
              value={galleryShare}
              onChange={(e) => setGalleryShare(e.target.value)}
              placeholder={smb.share || "Например, NAS"}
              spellCheck={false}
            />
            <span className="hint">
              Пусто — использовать основной Share. Укажите шар, чтобы все фото и
              резервные копии хранились на нём.
            </span>
          </div>
          <div className="field">
            <label>Папка галереи на NAS</label>
            <input
              value={galleryPath}
              onChange={(e) => setGalleryPath(e.target.value)}
              placeholder={`${smb.username || "Пользователь"}/Галерея`}
              spellCheck={false}
            />
            <span className="hint">
              Путь внутри выбранного шара. Внутри сохраняются папки альбомов телефона.
            </span>
          </div>
        </div>
        <div className="row" style={{ marginTop: 14 }}>
          <button className="btn" onClick={() => void test()} disabled={busy}>
            Проверить
          </button>
          <span className="spacer" />
          <button className="btn primary" onClick={() => void save()} disabled={busy}>
            {busy ? "Сохранение…" : "Сохранить"}
          </button>
        </div>
      </div>

      <div className="card">
        <h3>Обновления из GitHub</h3>
        <p className="muted small">
          Проверка релизов в репозитории. Когда репозиторий станет публичным, обновления
          будут доступны без токена. На Android приложение скачает APK и откроет системный
          установщик.
        </p>
        <div className="row" style={{ marginTop: 12 }}>
          <div className="field" style={{ flex: 1, minWidth: 220 }}>
            <label>Репозиторий (owner/repo)</label>
            <input
              value={updateRepo}
              onChange={(event) => setUpdateRepo(event.target.value)}
              className="mono"
              spellCheck={false}
            />
          </div>
          <button className="btn" onClick={() => void checkUpdates()} disabled={updateBusy}>
            {updateBusy ? "Проверка…" : "Проверить обновления"}
          </button>
          {updateInfo?.available && updateInfo.assetUrl && (
            <button className="btn primary" onClick={() => void installUpdate()} disabled={updateBusy}>
              Обновить
            </button>
          )}
        </div>
        {updateInfo && (
          <div className="small muted" style={{ marginTop: 10 }}>
            Установлено: <span className="mono">{updateInfo.currentVersion}</span> · Последняя:{" "}
            <span className="mono">{updateInfo.latestVersion}</span>
            {updateInfo.publishedAt ? ` · ${updateInfo.publishedAt}` : ""}
          </div>
        )}
        {updateMessage && (
          <div className={`${updateInfo?.available ? "success" : "hint"}`} style={{ marginTop: 10 }}>
            {updateMessage}
          </div>
        )}
        {updateInfo?.notes && (
          <pre className="hint" style={{ whiteSpace: "pre-wrap", marginTop: 10 }}>
            {updateInfo.notes}
          </pre>
        )}
      </div>

      <div className="card">
        <h3>Диагностика</h3>
        <p className="muted small">
          Отправляет файл с ошибками в выбранную папку на NAS. Работает на ПК и Android.
        </p>
        <div className="field" style={{ marginTop: 10 }}>
          <label>Папка для файла ошибок</label>
          <div className="row">
            <input
              value={diagnosticsPath}
              onChange={(event) => setDiagnosticsPath(event.target.value)}
              placeholder="Share/Diagnostics"
              spellCheck={false}
            />
            <button
              className="btn"
              onClick={() => setDiagnosticsPickerOpen(true)}
              disabled={diagnosticsBusy}
            >
              Выбрать папку
            </button>
          </div>
          <span className="hint">
            Если Share не задан, укажите путь вместе с шаром: Share/Папка.
          </span>
        </div>
        <div className="row" style={{ marginTop: 12 }}>
          <button
            className="btn primary"
            onClick={() => void sendErrorLog()}
            disabled={diagnosticsBusy}
          >
            {diagnosticsBusy ? "Отправка..." : "Отправить файл ошибок"}
          </button>
          {diagnosticsInfo && <span className="small muted">{diagnosticsInfo}</span>}
        </div>
      </div>

      <div className="card">
        <h3>О приложении</h3>
        <div className="small muted">
          ZeroSMB {config.platform === "android" ? "Android" : config.platform} · Rust + Tauri ·
          ZeroTier + SMB2/3
        </div>
        <div className="hint" style={{ marginTop: 8 }}>
          Пароль SMB хранится в системном хранилище секретов (на десктопе) или в приватном
          каталоге приложения (на Android). Пароль не попадает в файл основной конфигурации и
          не пишется в логи.
        </div>
      </div>
      <ColorPickerDialog
        open={themePickerOpen}
        initialColor={normalizeHex(themeColor)}
        onCancel={() => setThemePickerOpen(false)}
        onApply={(color) => {
          setThemeColor(color);
          applyTheme(color);
          setThemePickerOpen(false);
        }}
      />
      <FolderPickerDialog
        open={aiDenyPickerOpen}
        title="Запретить папку для ИИ"
        onCancel={() => setAiDenyPickerOpen(false)}
        onSelect={(path) => {
          setAiDenyPaths((current) =>
            current.includes(path) ? current : [...current, path],
          );
          setAiDenyPickerOpen(false);
        }}
      />
      <FolderPickerDialog
        open={diagnosticsPickerOpen}
        title="Папка для файла ошибок"
        onCancel={() => setDiagnosticsPickerOpen(false)}
        onSelect={(path) => {
          setDiagnosticsPath(path);
          setDiagnosticsPickerOpen(false);
        }}
      />
      <ConfirmDialog
        open={confirmUninstall}
        title="Удалить ZeroSMB из системы?"
        message="Будут удалены ярлыки, автозапуск и установленная копия приложения. Настройки, пароли и данные сохранятся."
        confirmLabel="Удалить из системы"
        busy={uninstallBusy}
        onCancel={() => setConfirmUninstall(false)}
        onConfirm={() => {
          setConfirmUninstall(false);
          void uninstall();
        }}
      >
        <label className="check-row" style={{ marginTop: 0 }}>
          <input
            type="checkbox"
            checked={deleteDataOnUninstall}
            onChange={(event) => setDeleteDataOnUninstall(event.target.checked)}
          />
          Также удалить кэш, настройки и пароли
        </label>
      </ConfirmDialog>
      <ConfirmDialog
        open={confirmClear}
        title="Очистить кэш и данные?"
        message="Будут удалены настройки, пароли, кэш, история синхронизации и временные файлы. Приложение перезапустится и попросит настроить подключение заново."
        confirmLabel="Очистить"
        busy={clearBusy}
        onCancel={() => setConfirmClear(false)}
        onConfirm={() => {
          setConfirmClear(false);
          void clearData();
        }}
      />
    </div>
  );
}
