import { useEffect, useState } from "react";
import { api, formatBytes } from "../api";
import type {
  PublicConfig,
  SyncJob,
  SyncMode,
  SyncProgress,
  SyncSummary,
} from "../types";

interface Props {
  android: boolean;
  config: PublicConfig;
  onConfigChange: (config: PublicConfig) => void;
}

const MODE_OPTIONS: { value: SyncMode; label: string }[] = [
  { value: "upload_manual", label: "Только по кнопке: локальная → NAS" },
  { value: "download_manual", label: "Только по кнопке: NAS → локальная" },
  { value: "upload_auto", label: "Автоматически: локальная → NAS" },
  { value: "download_auto", label: "Автоматически: NAS → локальная" },
  {
    value: "mirror_auto",
    label: "Автоматически: одинаково в обе стороны",
  },
];

const INTERVAL_OPTIONS = [1, 5, 15, 30, 60, 180, 360, 720, 1440];

function modeLabel(mode: SyncMode): string {
  return MODE_OPTIONS.find((option) => option.value === mode)?.label ?? mode;
}

function intervalLabel(minutes: number): string {
  if (minutes % 1440 === 0) return `${minutes / 1440} дн.`;
  if (minutes % 60 === 0) return `${minutes / 60} ч.`;
  return `${minutes} мин.`;
}

function runLabel(job: SyncJob): string {
  if (job.mode === "upload_manual") return "Синхронизировать на NAS";
  if (job.mode === "download_manual") return "Синхронизировать с NAS";
  return "Запустить сейчас";
}

export default function SyncView({ android, config, onConfigChange }: Props) {
  const [label, setLabel] = useState("");
  const [remote, setRemote] = useState("");
  const [localPath, setLocalPath] = useState("");
  const [treeUri, setTreeUri] = useState(config.androidTreeUri ?? "");
  const [treeName, setTreeName] = useState("");
  const [mode, setMode] = useState<SyncMode>("upload_manual");
  const [intervalMinutes, setIntervalMinutes] = useState(15);
  const [deleteEnabled, setDeleteEnabled] = useState(false);
  const [editingId, setEditingId] = useState("");
  const [remoteDirs, setRemoteDirs] = useState<string[]>([]);
  const [dirsLoading, setDirsLoading] = useState(false);
  const [error, setError] = useState("");
  const [info, setInfo] = useState("");
  const [busy, setBusy] = useState(false);
  const [runningId, setRunningId] = useState("");
  const [progress, setProgress] = useState<SyncProgress | null>(null);
  const [summary, setSummary] = useState<SyncSummary | null>(null);

  useEffect(() => {
    setTreeUri(config.androidTreeUri ?? "");
  }, [config.androidTreeUri]);

  const loadRemoteDirs = async () => {
    setDirsLoading(true);
    try {
      setRemoteDirs(await api.syncListRemoteDirs());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setDirsLoading(false);
    }
  };

  useEffect(() => {
    void loadRemoteDirs();
  }, []);

  const resetForm = () => {
    setEditingId("");
    setLabel("");
    setRemote("");
    setLocalPath("");
    setTreeUri(config.androidTreeUri ?? "");
    setTreeName("");
    setMode("upload_manual");
    setIntervalMinutes(15);
    setDeleteEnabled(false);
  };

  const pickLocal = async () => {
    setError("");
    try {
      const picked = await api.pickLocalFolder();
      if (picked) setLocalPath(picked);
    } catch (cause) {
      setError(String(cause));
    }
  };

  const pickAndroid = async () => {
    setError("");
    try {
      const picked = await api.androidPickTree();
      if (picked) {
        setTreeUri(picked);
        setTreeName(picked.split(":").pop() ?? "");
      }
    } catch (cause) {
      setError(String(cause));
    }
  };

  const edit = (job: SyncJob) => {
    setEditingId(job.id);
    setLabel(job.label);
    setRemote(job.remotePath);
    setMode(job.mode);
    setIntervalMinutes(job.intervalMinutes);
    setDeleteEnabled(job.deleteEnabled);
    if (job.source.kind === "localPath") {
      setLocalPath(job.source.value);
      setTreeName("");
    } else {
      setTreeUri(job.source.value);
      setTreeName(job.source.value.split(":").pop() ?? "");
    }
    setError("");
    setInfo("");
  };

  const save = async () => {
    setError("");
    setInfo("");
    if (!remote.trim()) {
      setError("Укажите папку на NAS");
      return;
    }
    if (android && !treeUri) {
      setError("Выберите папку Android");
      return;
    }
    if (!android && !localPath) {
      setError("Выберите локальную папку");
      return;
    }
    setBusy(true);
    try {
      const payload = {
        label: label.trim() || undefined,
        localPath: android ? undefined : localPath,
        androidTreeUri: android ? treeUri : undefined,
        remotePath: remote.trim(),
        mode,
        intervalMinutes,
        deleteEnabled,
      };
      if (editingId) {
        onConfigChange(await api.syncUpdateJob({ id: editingId, ...payload }));
        setInfo("Задание обновлено");
      } else {
        await api.syncAddJob(payload);
        onConfigChange(await api.getConfig());
        setInfo("Задание добавлено");
      }
      resetForm();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const toggleDelete = async (job: SyncJob) => {
    setError("");
    setBusy(true);
    try {
      onConfigChange(
        await api.syncUpdateJob({
          id: job.id,
          label: job.label,
          localPath: job.source.kind === "localPath" ? job.source.value : undefined,
          androidTreeUri:
            job.source.kind === "androidTree" ? job.source.value : undefined,
          remotePath: job.remotePath,
          mode: job.mode,
          intervalMinutes: job.intervalMinutes,
          deleteEnabled: !job.deleteEnabled,
        }),
      );
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (job: SyncJob) => {
    setError("");
    try {
      onConfigChange(await api.syncRemoveJob(job.id));
    } catch (cause) {
      setError(String(cause));
    }
  };

  const run = async (job: SyncJob) => {
    setRunningId(job.id);
    setProgress(null);
    setSummary(null);
    setError("");
    try {
      const result = await api.syncRunJob(job.id, setProgress);
      setSummary(result);
      onConfigChange(await api.getConfig());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setRunningId("");
    }
  };

  const describeLocal = (job: SyncJob) =>
    job.source.kind === "localPath"
      ? job.source.value
      : `Android: ${job.source.value.split(":").pop() ?? job.source.value}`;

  const isAuto = mode.endsWith("_auto");
  const availableModes = android
    ? MODE_OPTIONS.filter(
        (option) => option.value === "upload_manual" || option.value === "upload_auto",
      )
    : MODE_OPTIONS;

  return (
    <div className="stack">
      <div className="card">
        <h3>{editingId ? "Изменить задание" : "Новое задание синхронизации"}</h3>
        <div className="field" style={{ marginTop: 12 }}>
          <label>Название</label>
          <input
            value={label}
            onChange={(event) => setLabel(event.target.value)}
            placeholder="Например, Фото с телефона"
          />
        </div>
        <div className="field" style={{ marginTop: 12 }}>
          <label>Папка на NAS</label>
          <input
            value={remote}
            onChange={(event) => setRemote(event.target.value)}
            placeholder="Backup/Phone"
            spellCheck={false}
          />
          <div className="row" style={{ marginTop: 6 }}>
            <select
              value={remoteDirs.includes(remote) ? remote : ""}
              onChange={(event) => {
                if (event.target.value !== "") setRemote(event.target.value);
              }}
              style={{ flex: 1, minWidth: 0 }}
            >
              <option value="">Выбрать папку на NAS…</option>
              {remoteDirs.map((dir) => (
                <option key={dir} value={dir}>
                  {dir}
                </option>
              ))}
            </select>
            <button
              className="btn ghost"
              onClick={() => void loadRemoteDirs()}
              disabled={dirsLoading}
            >
              {dirsLoading ? "…" : "Обновить"}
            </button>
          </div>
          <span className="hint">
            Можно выбрать папку из списка или ввести путь вручную.
          </span>
        </div>
        <div className="field" style={{ marginTop: 12 }}>
          <label>Локальная папка</label>
          {android ? (
            <div className="row">
              <input
                value={treeName || treeUri}
                readOnly
                placeholder="Папка Android не выбрана"
                spellCheck={false}
              />
              <button className="btn" onClick={() => void pickAndroid()}>
                Выбрать
              </button>
            </div>
          ) : (
            <div className="row">
              <input
                value={localPath}
                onChange={(event) => setLocalPath(event.target.value)}
                placeholder="C:\Users\Имя\Documents\ZeroSMB"
                spellCheck={false}
              />
              <button className="btn" onClick={() => void pickLocal()}>
                Выбрать
              </button>
            </div>
          )}
        </div>
        <div className="field" style={{ marginTop: 12 }}>
          <label>Режим синхронизации</label>
          <select
            value={mode}
            onChange={(event) => setMode(event.target.value as SyncMode)}
          >
            {availableModes.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          {android && (
            <span className="hint">
              На Android доступна синхронизация локальной папки на NAS.
            </span>
          )}
        </div>
        {isAuto && (
          <div className="field" style={{ marginTop: 12, maxWidth: 260 }}>
            <label>Частота автоматической синхронизации</label>
            <select
              value={intervalMinutes}
              onChange={(event) => setIntervalMinutes(Number(event.target.value))}
            >
              {INTERVAL_OPTIONS.map((minutes) => (
                <option key={minutes} value={minutes}>
                  {intervalLabel(minutes)}
                </option>
              ))}
            </select>
          </div>
        )}
        <label className="check-row" style={{ marginTop: 12 }}>
          <input
            type="checkbox"
            checked={deleteEnabled}
            onChange={(event) => setDeleteEnabled(event.target.checked)}
          />
          Удалять файлы, удалённые на другой стороне
        </label>
        <div className="row" style={{ marginTop: 14 }}>
          <button className="btn primary" onClick={() => void save()} disabled={busy}>
            {busy ? "Сохранение…" : editingId ? "Сохранить изменения" : "Добавить задание"}
          </button>
          {editingId && (
            <button className="btn" onClick={resetForm} disabled={busy}>
              Отмена
            </button>
          )}
          <span className="spacer" />
          {remoteDirs.length === 0 && !dirsLoading && (
            <span className="hint">Список папок NAS пуст или недоступен</span>
          )}
        </div>
        {error && <div className="error" style={{ marginTop: 12 }}>{error}</div>}
        {info && <div className="success" style={{ marginTop: 12 }}>{info}</div>}
      </div>

      <div className="card">
        <h3>Задания</h3>
        {config.syncJobs.length === 0 && (
          <div className="empty">Пока нет заданий синхронизации</div>
        )}
        {config.syncJobs.map((job) => (
          <div className="job" key={job.id}>
            <div className="job-head">
              <span className="nav-icon">
                {job.source.kind === "androidTree" ? "AND" : "PC"}
              </span>
              <strong>{job.label}</strong>
              <span className="chip mono">{job.remotePath}</span>
              <button
                className="btn"
                onClick={() => void run(job)}
                disabled={runningId === job.id || busy}
              >
                {runningId === job.id ? "Синхронизация…" : runLabel(job)}
              </button>
              <button
                className="btn"
                onClick={() => edit(job)}
                disabled={runningId === job.id}
              >
                Изменить
              </button>
              <button
                className="btn danger ghost"
                onClick={() => void remove(job)}
                disabled={runningId === job.id}
              >
                Удалить
              </button>
            </div>
            <div className="hint mono" style={{ marginTop: 6 }}>
              {describeLocal(job)}
            </div>
            <div className="row small muted" style={{ marginTop: 6 }}>
              <span>{modeLabel(job.mode)}</span>
              {job.mode.endsWith("_auto") && (
                <span>· каждые {intervalLabel(job.intervalMinutes)}</span>
              )}
              <span className="spacer" />
              <label className="check-row" style={{ marginTop: 0 }}>
                <input
                  type="checkbox"
                  checked={job.deleteEnabled}
                  onChange={() => void toggleDelete(job)}
                  disabled={busy || runningId === job.id}
                />
                Удаление файлов
              </label>
            </div>
            {runningId === job.id && progress && (
              <div style={{ marginTop: 10 }}>
                <div className="progress">
                  <div
                    style={{
                      width: `${progress.total ? (progress.current / progress.total) * 100 : 0}%`,
                    }}
                  />
                </div>
                <div className="row small muted" style={{ marginTop: 5 }}>
                  <span>
                    {progress.current}/{progress.total}
                  </span>
                  <span className="spacer" />
                  <span>
                    {formatBytes(progress.bytes)} · пропущено: {progress.skipped} · ошибок:{" "}
                    {progress.failed}
                  </span>
                </div>
                {progress.file && <div className="hint mono">{progress.file}</div>}
              </div>
            )}
          </div>
        ))}
        {summary && (
          <div className="success" style={{ marginTop: 12 }}>
            Загружено: {summary.uploaded}, скачано: {summary.downloaded}, удалено:{" "}
            {summary.deleted}, пропущено: {summary.skipped}, ошибок: {summary.failed},{" "}
            {formatBytes(summary.bytes)}
            {summary.errors.length > 0 && (
              <div style={{ marginTop: 6 }}>{summary.errors.slice(0, 8).join("\n")}</div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
