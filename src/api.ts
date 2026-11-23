import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
  AiStatus,
  ArchiveEntry,
  ConnectionMode,
  DocumentPreview,
  ExternalFile,
  FileEntry,
  FileKind,
  GalleryItem,
  GalleryList,
  DeleteResult,
  MediaRef,
  PlanetInfo,
  PublicConfig,
  RuntimeInfo,
  SettingsInput,
  SetupInput,
  SmbConfig,
  SyncJob,
  SyncMode,
  SyncProgress,
  SyncSummary,
  TransferSnapshot,
  UpdateInfo,
  WireGuardStatus,
  UploadResult,
  DownloadResult,
  ZerotierMode,
  ZtStatus,
} from "./types";

export const api = {
  runtimeInfo: () => invoke<RuntimeInfo>("runtime_info"),
  requestNotificationPermission: () =>
    invoke<void>("request_notification_permission"),
  testNotification: () => invoke<string>("test_notification"),
  getConfig: () => invoke<PublicConfig>("get_config"),
  testConnection: (
    smb: SmbConfig,
    password: string,
    zerotierNetworkId?: string,
    connectionMode?: ConnectionMode,
    zerotierMode?: ZerotierMode,
  ) =>
    invoke<void>("test_connection", {
      smb,
      password,
      zerotierNetworkId,
      connectionMode,
      zerotierMode,
    }),
  testCurrentConnection: () => invoke<void>("test_current_connection"),
  testConnectionSaved: (
    smb: SmbConfig,
    zerotierNetworkId?: string,
    connectionMode?: ConnectionMode,
    zerotierMode?: ZerotierMode,
  ) =>
    invoke<void>("test_connection_saved", {
      smb,
      zerotierNetworkId,
      connectionMode,
      zerotierMode,
    }),
  saveSetup: (input: SetupInput) => invoke<PublicConfig>("save_setup", { input }),
  saveSettings: (input: SettingsInput) =>
    invoke<PublicConfig>("save_settings", { input }),
  pickLocalFolder: () => invoke<string | null>("pick_local_folder"),
  smbList: (path: string) => invoke<FileEntry[]>("smb_list", { path }),
  selectSmbShare: (share: string) => invoke<PublicConfig>("select_smb_share", { share }),
  clearSmbShare: () => invoke<PublicConfig>("clear_smb_share"),
  sendErrorLog: (remoteDir: string) =>
    invoke<string>("send_error_log", { remoteDir }),
  deletePaths: (paths: string[]) => invoke<DeleteResult>("delete_paths", { paths }),
  createFolder: (path: string) => invoke<void>("create_folder", { path }),
  uploadFiles: (
    remoteDir: string,
    onProgress: (progress: SyncProgress) => void,
    mediaOnly = false,
  ) => {
    const channel = new Channel<SyncProgress>();
    channel.onmessage = onProgress;
    return invoke<UploadResult>("upload_files", {
      remoteDir,
      mediaOnly,
      onProgress: channel,
    });
  },
  uploadFolder: (
    remoteDir: string,
    onProgress: (progress: SyncProgress) => void,
  ) => {
    const channel = new Channel<SyncProgress>();
    channel.onmessage = onProgress;
    return invoke<UploadResult>("upload_folder", {
      remoteDir,
      onProgress: channel,
    });
  },
  downloadFile: (
    path: string,
    relativeDir?: string,
    conflictPolicy?: "skip" | "rename",
    onProgress?: (progress: SyncProgress) => void,
  ) => {
    const channel = new Channel<SyncProgress>();
    if (onProgress) channel.onmessage = onProgress;
    return invoke<string>("download_file", {
      path,
      relativeDir,
      conflictPolicy,
      onProgress: channel,
    });
  },
  readTextFile: (path: string) => invoke<string>("read_text_file", { path }),
  readFileBytes: (path: string) => invoke<ArrayBuffer>("read_file_bytes", { path }),
  previewDocument: (path: string) =>
    invoke<DocumentPreview>("preview_document", { path }),
  archiveList: (path: string) =>
    invoke<ArchiveEntry[]>("archive_list", { path }),
  archiveRead: (path: string, index: number) =>
    invoke<ArrayBuffer>("archive_read", { path, index }),
  archivePreviewEntry: (path: string, index: number) =>
    invoke<DocumentPreview>("archive_preview_entry", { path, index }),
  archiveSaveEntry: (path: string, index: number) =>
    invoke<string>("archive_save_entry", { path, index }),
  archiveOpenEntry: (path: string, index: number) =>
    invoke<string>("archive_open_entry", { path, index }),
  openRemoteFile: (path: string) =>
    invoke<ExternalFile>("open_remote_file", { path }),
  externalFileSignature: (localPath: string) =>
    invoke<string>("external_file_signature", { localPath }),
  saveExternalFile: (remotePath: string, localPath: string) =>
    invoke<string>("save_external_file", { remotePath, localPath }),
  readMediaBytes: (path: string) =>
    invoke<ArrayBuffer>("read_media_bytes", { path }),
  ensureGalleryFolder: (path: string) =>
    invoke<boolean>("ensure_gallery_folder", { path }),
  galleryList: () => invoke<GalleryList>("gallery_list"),
  galleryThumbnail: (uri: string, size = 256) =>
    invoke<string>("gallery_thumbnail", { uri, size }),
  galleryExport: (uri: string, name: string, mime: string) =>
    invoke<MediaRef>("gallery_export", { uri, name, mime }),
  downloadGallery: (
    remotePath: string,
    conflictPolicy: "skip" | "rename",
    onProgress: (progress: SyncProgress) => void,
  ) => {
    const channel = new Channel<SyncProgress>();
    channel.onmessage = onProgress;
    return invoke<DownloadResult>("download_gallery", {
      remotePath,
      conflictPolicy,
      onProgress: channel,
    });
  },
  syncGallery: (
    remotePath: string,
    items: GalleryItem[] | undefined,
    conflictPolicy: "skip" | "rename",
    onProgress: (progress: SyncProgress) => void,
  ) => {
    const channel = new Channel<SyncProgress>();
    channel.onmessage = onProgress;
    return invoke<SyncSummary>("sync_gallery", {
      remotePath,
      items,
      conflictPolicy,
      onProgress: channel,
    });
  },
  androidPickTree: () => invoke<string | null>("android_pick_tree"),
  androidTreeUri: () => invoke<string | null>("android_tree_uri"),
  syncAddJob: (payload: {
    label?: string;
    localPath?: string;
    androidTreeUri?: string;
    remotePath: string;
    mode: SyncMode;
    intervalMinutes: number;
    deleteEnabled: boolean;
  }) => invoke<SyncJob>("sync_add_job", payload),
  syncUpdateJob: (payload: {
    id: string;
    label?: string;
    localPath?: string;
    androidTreeUri?: string;
    remotePath: string;
    mode: SyncMode;
    intervalMinutes: number;
    deleteEnabled: boolean;
  }) => invoke<PublicConfig>("sync_update_job", payload),
  syncRemoveJob: (id: string) =>
    invoke<PublicConfig>("sync_remove_job", { id }),
  syncListRemoteDirs: () => invoke<string[]>("sync_list_remote_dirs"),
  installSystem: (targetDir: string) =>
    invoke<string>("install", { targetDir }),
  uninstallSystem: (deleteData: boolean) =>
    invoke<string>("uninstall", { deleteData }),
  wireguardStatus: () => invoke<WireGuardStatus>("wireguard_status"),
  wireguardPickConfig: () =>
    invoke<string | null>("wireguard_pick_config"),
  wireguardConnect: () => invoke<string>("wireguard_connect"),
  wireguardDisconnect: () => invoke<string>("wireguard_disconnect"),
  wireguardOpenHelp: () => invoke<void>("wireguard_open_help"),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  transferSnapshot: () => invoke<TransferSnapshot>("transfer_snapshot"),
  transferClearCompleted: () =>
    invoke<TransferSnapshot>("transfer_clear_completed"),
  clearAppData: () => invoke<string>("clear_app_data"),
  syncRunJob: (id: string, onProgress: (progress: SyncProgress) => void) => {
    const channel = new Channel<SyncProgress>();
    channel.onmessage = onProgress;
    return invoke<SyncSummary>("sync_run_job", { id, onProgress: channel });
  },
  ztStatus: (zerotierMode?: ZerotierMode) =>
    invoke<ZtStatus>("zt_status", { zerotierMode }),
  ztJoin: (networkId: string, zerotierMode?: ZerotierMode) =>
    invoke<ZtStatus>("zt_join", { networkId, zerotierMode }),
  ztLeave: (networkId: string, zerotierMode?: ZerotierMode) =>
    invoke<ZtStatus>("zt_leave", { networkId, zerotierMode }),
  ztOpenAndroid: (networkId: string) =>
    invoke<{ installed: boolean; opened: boolean; copied: boolean }>(
      "zt_open_android",
      { networkId },
    ),
  ztOpenHelp: () => invoke<void>("zt_open_help"),
  getPlanet: (zerotierMode?: ZerotierMode) =>
    invoke<PlanetInfo>("get_planet", { zerotierMode }),
  pickPlanet: (zerotierMode?: ZerotierMode) =>
    invoke<PlanetInfo>("pick_planet", { zerotierMode }),
  clearPlanet: (zerotierMode?: ZerotierMode) =>
    invoke<PlanetInfo>("clear_planet", { zerotierMode }),
  checkUpdate: (repo?: string) => invoke<UpdateInfo>("check_update", { repo }),
  installUpdate: (url: string, filename: string) =>
    invoke<string>("install_update", { url, filename }),
};

export function mediaUrl(path: string): string {
  return convertFileSrc(path, "media");
}

export function mimeFor(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const types: Record<string, string> = {
    gif: "image/gif",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    png: "image/png",
    webp: "image/webp",
    bmp: "image/bmp",
    avif: "image/avif",
    heic: "image/heic",
    heif: "image/heif",
    svg: "image/svg+xml",
    mp4: "video/mp4",
    m4v: "video/mp4",
    webm: "video/webm",
    mkv: "video/x-matroska",
    mov: "video/quicktime",
    avi: "video/x-msvideo",
    "3gp": "video/3gpp",
    ogv: "video/ogg",
  };
  return types[ext] ?? "application/octet-stream";
}
export function kindOfName(name: string): FileKind {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (["zip", "rar"].includes(ext)) return "archive";
  if (["docx", "xlsx", "pptx", "odt", "ods", "odp", "epub"].includes(ext)) {
    return "document";
  }
  if (ext === "gif") return "gif";
  if (["mp4", "m4v", "webm", "mkv", "mov", "avi", "3gp", "ogv"].includes(ext)) {
    return "video";
  }
  if (["jpg", "jpeg", "png", "webp", "bmp", "avif", "heic", "heif", "svg"].includes(ext)) {
    return "image";
  }
  if (ext === "pdf") return "pdf";
  if (
    [
      "txt", "log", "md", "markdown", "json", "xml", "csv", "tsv", "ini", "conf",
      "cfg", "yml", "yaml", "toml", "rs", "js", "mjs", "ts", "tsx", "jsx",
      "html", "htm", "css", "scss", "py", "sh", "bat", "ps1", "sql", "env",
      "gitignore", "nfo", "srt", "vtt",
    ].includes(ext)
  ) {
    return "text";
  }
  return "file";
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(value >= 10 || index === 0 ? 0 : 1)} ${units[index]}`;
}

export function formatDate(unixSeconds: number): string {
  if (!unixSeconds) return "";
  return new Date(unixSeconds * 1000).toLocaleString();
}
