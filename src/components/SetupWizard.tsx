import { useEffect, useState } from "react";
import { api } from "../api";
import CloudMark from "./CloudMark";
import type {
  ConnectionMode,
  PlanetInfo,
  PublicConfig,
  RuntimeInfo,
  SmbConfig,
  WireGuardStatus,
  ZtStatus,
} from "../types";

interface Props {
  onDone: (config: PublicConfig) => void;
}

const emptySmb: SmbConfig = {
  host: "",
  share: "",
  port: 445,
  domain: "",
  username: "",
  basePath: "",
};

export default function SetupWizard({ onDone }: Props) {
  const [step, setStep] = useState(0);
  const [mode, setMode] = useState<ConnectionMode>("local");
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
  const [networkId, setNetworkId] = useState("");
  const [zt, setZt] = useState<ZtStatus | null>(null);
  const [ztBusy, setZtBusy] = useState(false);
  const [ztMessage, setZtMessage] = useState("");
  const [planetInfo, setPlanetInfo] = useState<PlanetInfo | null>(null);
  const [planetBusy, setPlanetBusy] = useState(false);
  const [wireguard, setWireguard] = useState<WireGuardStatus | null>(null);
  const [wireguardBusy, setWireguardBusy] = useState(false);
  const [wireguardMessage, setWireguardMessage] = useState("");
  const [smb, setSmb] = useState<SmbConfig>(emptySmb);
  const [password, setPassword] = useState("");
  const [galleryPath, setGalleryPath] = useState("");
  const [systemInstallDir, setSystemInstallDir] = useState("");
  const [systemAutostart, setSystemAutostart] = useState(true);
  const [systemStartMenu, setSystemStartMenu] = useState(true);
  const [systemDesktopShortcut, setSystemDesktopShortcut] = useState(false);
  const [systemTray, setSystemTray] = useState(true);
  const [downloadDir, setDownloadDir] = useState("");
  const [downloadAutoSave, setDownloadAutoSave] = useState(true);
  const [downloadKeepStructure, setDownloadKeepStructure] = useState(true);
  const [systemNotifications, setSystemNotifications] = useState(true);
  const [notifyDownloads, setNotifyDownloads] = useState(true);
  const [notifyUploads, setNotifyUploads] = useState(true);
  const [notifySync, setNotifySync] = useState(true);
  const [notifyErrors, setNotifyErrors] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [info, setInfo] = useState("");

  const isAndroid = runtime?.android ?? false;
  const isLinux = runtime?.platform === "linux";

  useEffect(() => {
    void api.runtimeInfo().then(setRuntime).catch(() => undefined);
    void api.ztStatus().then(setZt).catch(() => undefined);
    void api.getPlanet().then(setPlanetInfo).catch(() => undefined);
    void api.wireguardStatus().then(setWireguard).catch(() => undefined);
  }, []);

  const updateSmb = <K extends keyof SmbConfig>(key: K, value: SmbConfig[K]) => {
    setSmb((current) => ({ ...current, [key]: value }));
  };

  const chooseMode = (next: ConnectionMode) => {
    setError("");
    setInfo("");
    setMode(next);
    setStep(next === "local" ? 2 : 1);
  };

  const connectZt = async (): Promise<boolean> => {
    setError("");
    setInfo("");
    if (!networkId.trim()) {
      setError("Введите Network ID из ZeroTier Central");
      return false;
    }
    setZtBusy(true);
    try {
      const status = await api.ztJoin(networkId.trim());
      setZt(status);
      setZtMessage(status.message);
      setInfo(status.message);
      return true;
    } catch (cause) {
      setError(String(cause));
      return false;
    } finally {
      setZtBusy(false);
    }
  };

  const pickPlanet = async () => {
    setPlanetBusy(true);
    setError("");
    setInfo("");
    try {
      const info = await api.pickPlanet();
      setPlanetInfo(info);
      setInfo("Свой planet выбран. Переподключите ZeroTier, чтобы применить.");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPlanetBusy(false);
    }
  };

  const clearPlanet = async () => {
    setPlanetBusy(true);
    setError("");
    setInfo("");
    try {
      const info = await api.clearPlanet();
      setPlanetInfo(info);
      setInfo(info.message);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPlanetBusy(false);
    }
  };

  const refreshWireGuard = async () => {
    setWireguard(await api.wireguardStatus());
  };

  const pickWireGuard = async () => {
    setWireguardBusy(true);
    setError("");
    setWireguardMessage("");
    try {
      const path = await api.wireguardPickConfig();
      if (path) {
        await refreshWireGuard();
        setWireguardMessage("Конфигурация выбрана");
      }
    } catch (cause) {
      setError(String(cause));
    } finally {
      setWireguardBusy(false);
    }
  };

  const connectWireGuard = async (): Promise<boolean> => {
    setWireguardBusy(true);
    setError("");
    setWireguardMessage("");
    try {
      setWireguardMessage(await api.wireguardConnect());
      await refreshWireGuard();
      return true;
    } catch (cause) {
      setError(String(cause));
      return false;
    } finally {
      setWireguardBusy(false);
    }
  };

  const disconnectWireGuard = async () => {
    setWireguardBusy(true);
    setError("");
    setWireguardMessage("");
    try {
      setWireguardMessage(await api.wireguardDisconnect());
      await refreshWireGuard();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setWireguardBusy(false);
    }
  };

  const nextFromVpn = async () => {
    setError("");
    setInfo("");
    if (mode === "zerotier") {
      if (await connectZt()) setStep(2);
      return;
    }
    if (mode === "wireguard") {
      if (!wireguard?.configured) {
        setError("Выберите конфигурацию WireGuard (.conf)");
        return;
      }
      if (wireguard.connected || (await connectWireGuard())) setStep(2);
    }
  };

  const backFromSmb = () => {
    setError("");
    setInfo("");
    setStep(mode === "local" ? 0 : 1);
  };

  const test = async () => {
    setError("");
    setInfo("");
    setBusy(true);
    try {
      if (mode === "wireguard" && !wireguard?.connected) {
        if (!(await connectWireGuard())) return;
      }
      await api.testConnection(smb, password, networkId.trim(), mode);
      setInfo("SMB-соединение установлено, доступ к ресурсу есть.");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const save = async (skipIntegration = false) => {
    setError("");
    setInfo("");
    setBusy(true);
    try {
      if (mode === "wireguard" && !wireguard?.connected) {
        if (!(await connectWireGuard())) return;
      }
      const installDir = skipIntegration ? "" : systemInstallDir.trim();
      const config = await api.saveSetup({
        zerotierNetworkId: networkId.trim(),
        connectionMode: mode,
        smb,
        password,
        remoteGalleryPath: galleryPath.trim(),
        systemInstallDir: isAndroid ? undefined : installDir,
        systemAutostart: skipIntegration ? false : systemAutostart,
        systemStartMenu: skipIntegration ? false : systemStartMenu,
        systemDesktopShortcut: skipIntegration ? false : systemDesktopShortcut,
        systemTray: skipIntegration ? false : systemTray,
        systemNotifications,
        notifyDownloads,
        notifyUploads,
        notifySync,
        notifyErrors,
        downloadDir: isAndroid ? undefined : downloadDir.trim(),
        downloadAutoSave,
        downloadKeepStructure,
      });
      if (!isAndroid && installDir) {
        setInfo(await api.installSystem(installDir));
        onDone(await api.getConfig());
      } else {
        onDone(config);
      }
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="setup-wrap">
      <div className="card setup-card">
        <div className="brand" style={{ padding: 0, marginBottom: 18 }}>
          <div className="brand-mark">
            <CloudMark />
          </div>
          <div>
            <h1>ZeroSMB</h1>
            <small>Первый запуск: подключение, NAS и доступ к файлам</small>
          </div>
        </div>

        <div className="steps">
          <div className={`step-dot ${step >= 0 ? "active" : ""}`} />
          <div className={`step-dot ${step >= 1 ? "active" : ""}`} />
          <div className={`step-dot ${step >= 2 ? "active" : ""}`} />
          {!isAndroid && <div className={`step-dot ${step >= 3 ? "active" : ""}`} />}
        </div>

        {step === 0 && (
          <div className="stack">
            <div>
              <h3>1. Способ подключения</h3>
              <p className="muted small">
                Выберите, как ZeroSMB будет добираться до SMB-сервера. VPN встроен в
                приложение, отдельная программа не нужна.
              </p>
            </div>
            <div className="setup-choices">
              <button className="setup-choice" onClick={() => chooseMode("zerotier")}>
                <span className="setup-choice-title">ZeroTier</span>
                <span className="setup-choice-text">
                  Виртуальная сеть через Network ID. Устройство нужно авторизовать в
                  ZeroTier Central.
                </span>
              </button>
              <button className="setup-choice" onClick={() => chooseMode("wireguard")}>
                <span className="setup-choice-title">WireGuard</span>
                <span className="setup-choice-text">
                  Собственный WireGuard-сервер. Подключение по файлу конфигурации
                  .conf.
                </span>
              </button>
              <button className="setup-choice" onClick={() => chooseMode("local")}>
                <span className="setup-choice-title">Local</span>
                <span className="setup-choice-text">
                  Прямое подключение по локальной сети или IP без VPN.
                </span>
              </button>
            </div>
          </div>
        )}

        {step === 1 && mode === "zerotier" && (
          <div className="stack">
            <div>
              <h3>2. Сеть ZeroTier</h3>
              <p className="muted small">
                Укажите Network ID вашей сети. ZeroTier встроен прямо в ZeroSMB на всех
                платформах. Не забудьте авторизовать устройство в ZeroTier Central.
              </p>
            </div>
            <div className="field">
              <label>ZeroTier Network ID</label>
              <input
                value={networkId}
                onChange={(event) => setNetworkId(event.target.value)}
                placeholder="8056c2e21c000001"
                spellCheck={false}
                autoCapitalize="none"
                autoCorrect="off"
              />
            </div>
            <div className="row">
              <button className="btn primary" onClick={() => void connectZt()} disabled={ztBusy}>
                {ztBusy ? "Подключение…" : "Подключить сеть"}
              </button>
              {!isAndroid && (
                <button className="btn ghost" onClick={() => void api.ztOpenHelp()}>
                  Скачать ZeroTier
                </button>
              )}
              <span className="spacer" />
              <button className="btn ghost" onClick={() => setStep(0)}>
                Назад
              </button>
              <button className="btn" onClick={() => void nextFromVpn()}>
                Далее
              </button>
            </div>

            <div className="field" style={{ marginTop: 4 }}>
              <label>Корневой сервер ZeroTier (planet)</label>
              <span className="hint">
                {planetInfo?.configured
                  ? `Свой planet: ${planetInfo.size} байт`
                  : "Используются встроенные корни ZeroTier"}
              </span>
              <div className="row" style={{ marginTop: 6 }}>
                <button
                  className="btn primary"
                  onClick={() => void pickPlanet()}
                  disabled={planetBusy}
                >
                  {planetBusy ? "Чтение…" : "Выбрать planet"}
                </button>
                {planetInfo?.configured && (
                  <button
                    className="btn danger ghost"
                    onClick={() => void clearPlanet()}
                    disabled={planetBusy}
                  >
                    Убрать
                  </button>
                )}
                {planetInfo?.configured && (
                  <button
                    className="btn"
                    onClick={() => void connectZt()}
                    disabled={ztBusy || !networkId.trim()}
                  >
                    Переподключить
                  </button>
                )}
              </div>
              <span className="hint">
                Необязательно. Свой planet заменяет корневые серверы ZeroTier и
                применяется после переподключения сети.
              </span>
            </div>
            {ztMessage && <div className="hint">{ztMessage}</div>}
            {zt?.address && <div className="hint mono">Node ID: {zt.address}</div>}
            {zt && zt.networks.length > 0 && (
              <div>
                <div className="small muted" style={{ marginBottom: 6 }}>
                  Сети ZeroTier
                </div>
                {zt.networks.map((network) => (
                  <div className="zt-network" key={network.id}>
                    <div>
                      <div className="mono">{network.id}</div>
                      <div className="small muted">{network.name || "Без названия"}</div>
                    </div>
                    <span className={`chip ${network.status === "OK" ? "ok" : "warn"}`}>
                      {network.status}
                    </span>
                  </div>
                ))}
              </div>
            )}
            {zt && !zt.available && !isAndroid && (
              <div className="hint">
                Встроенный ZeroTier ещё не запущен. Нажмите «Подключить сеть» — приложение
                поднимет его автоматически.
              </div>
            )}
          </div>
        )}

        {step === 1 && mode === "wireguard" && (
          <div className="stack">
            <div>
              <h3>2. WireGuard</h3>
              <p className="muted small">
                Выберите файл .conf с ключами и адресом вашего WireGuard-сервера.
                Туннель работает внутри ZeroSMB, отдельное приложение и права
                администратора не нужны.
              </p>
            </div>
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
            </div>
            <div className="row">
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
              {!isAndroid && (
                <button className="btn ghost" onClick={() => void api.wireguardOpenHelp()}>
                  Помощь
                </button>
              )}
              <span className="spacer" />
              <button className="btn ghost" onClick={() => setStep(0)}>
                Назад
              </button>
              <button className="btn" onClick={() => void nextFromVpn()} disabled={wireguardBusy}>
                Далее
              </button>
            </div>
            {wireguard?.message && <div className="hint">{wireguard.message}</div>}
            {wireguardMessage && <div className="success">{wireguardMessage}</div>}
          </div>
        )}

        {step === 2 && (
          <div className="stack">
            <div>
              <h3>3. Доступ к NAS (SMB/CIFS)</h3>
              <p className="muted small">
                Укажите данные OpenMediaVault. Права определяет сам NAS: приложение
                аутентифицируется от вашего пользователя SMB.
              </p>
              <div className="hint">
                {mode === "local" &&
                  "Локальный режим: укажите LAN-имя или IP NAS. Телефон и NAS должны быть в одной сети."}
                {mode === "zerotier" &&
                  "ZeroTier: укажите IP или имя NAS внутри вашей сети ZeroTier."}
                {mode === "wireguard" &&
                  "WireGuard: укажите IP или имя NAS внутри вашей WireGuard-сети. Для внутренних имён используйте IP."}
              </div>
            </div>
            <div className="grid two">
              <div className="field">
                <label>Host / IP NAS</label>
                <input
                  value={smb.host}
                  onChange={(event) => updateSmb("host", event.target.value)}
                  placeholder="192.168.191.10 или nas.local"
                  spellCheck={false}
                />
              </div>
              <div className="field">
                <label>SMB share</label>
                <input
                  value={smb.share}
                  onChange={(event) => updateSmb("share", event.target.value)}
                  placeholder="media"
                  spellCheck={false}
                />
              </div>
              <div className="field">
                <label>Порт</label>
                <input
                  type="number"
                  value={smb.port}
                  onChange={(event) => updateSmb("port", Number(event.target.value) || 445)}
                />
              </div>
              <div className="field">
                <label>Домен (если нужен)</label>
                <input
                  value={smb.domain}
                  onChange={(event) => updateSmb("domain", event.target.value)}
                  placeholder="WORKGROUP"
                  spellCheck={false}
                />
              </div>
              <div className="field">
                <label>Пользователь SMB</label>
                <input
                  value={smb.username}
                  onChange={(event) => updateSmb("username", event.target.value)}
                  placeholder="user"
                  autoCapitalize="none"
                  spellCheck={false}
                />
              </div>
              <div className="field">
                <label>Пароль SMB</label>
                <input
                  type="password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  autoComplete="current-password"
                />
              </div>
              <div className="field">
                <label>Базовая папка на share (необязательно)</label>
                <input
                  value={smb.basePath}
                  onChange={(event) => updateSmb("basePath", event.target.value)}
                  placeholder="Files"
                  spellCheck={false}
                />
              </div>
              <div className="field">
                <label>Папка для галереи на NAS</label>
                <input
                  value={galleryPath}
                  onChange={(event) => setGalleryPath(event.target.value)}
                  placeholder="Пользователь/Галерея"
                  spellCheck={false}
                />
              </div>
            </div>
            <div className="row">
              <button className="btn" onClick={test} disabled={busy}>
                Проверить соединение
              </button>
              <span className="spacer" />
              <button className="btn ghost" onClick={backFromSmb} disabled={busy}>
                Назад
              </button>
              <button
                className="btn primary"
                onClick={() => (isAndroid ? void save() : setStep(3))}
                disabled={busy}
              >
                {busy ? "Сохранение…" : isAndroid ? "Сохранить и войти" : "Далее"}
              </button>
            </div>
            <div className="hint">
              Пароль хранится в системном хранилище (Windows Credential Manager / Secret
              Service). На Android — в приватном каталоге приложения, недоступном другим
              приложениям.
            </div>
          </div>
        )}

        {!isAndroid && step === 3 && (
          <div className="stack">
            <div>
              <h3>4. Интеграция с системой</h3>
              <p className="muted small">
                {isLinux
                  ? "ZeroSMB уже интегрирован в систему через пакет. Настройте автозапуск, работу в трее и уведомления."
                  : "Можно установить ZeroSMB в системную папку, включить автозапуск, работу в трее и уведомления. Все параметры потом можно изменить в настройках."}
              </p>
            </div>
            {!isLinux && (
              <div className="field">
                <label>Куда установить</label>
                <div className="row">
                  <input
                    value={systemInstallDir}
                    onChange={(event) => setSystemInstallDir(event.target.value)}
                    placeholder="Оставьте пустым, чтобы не устанавливать"
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
                  Если указать папку, приложение скопируется туда и перезапустится.
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
            <div className="card" style={{ marginTop: 4, background: "rgba(6, 10, 22, 0.45)" }}>
              <h3>Загрузки на ПК</h3>
              <div className="field">
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
                    onChange={(event) => setDownloadKeepStructure(event.target.checked)}
                  />
                  Сохранять структуру папок
                </label>
              </div>
            </div>
            <label className="check-row">
              <input
                type="checkbox"
                checked={systemNotifications}
                onChange={(event) => setSystemNotifications(event.target.checked)}
              />
              Показывать системные уведомления
            </label>
            {systemNotifications && (
              <div className="row">
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
            <div className="row">
              <button className="btn ghost" onClick={() => setStep(2)} disabled={busy}>
                Назад
              </button>
              <button className="btn" onClick={() => void save(true)} disabled={busy}>
                Пропустить интеграцию
              </button>
              <span className="spacer" />
              <button className="btn primary" onClick={() => void save()} disabled={busy}>
                {busy
                  ? "Сохранение…"
                  : systemInstallDir.trim()
                    ? "Сохранить и установить"
                    : "Сохранить и войти"}
              </button>
            </div>
          </div>
        )}

        {error && <div className="error" style={{ marginTop: 14 }}>{error}</div>}
        {info && <div className="success" style={{ marginTop: 14 }}>{info}</div>}
      </div>
    </div>
  );
}
