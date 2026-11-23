package com.zerosmb.app

import android.Manifest
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaScannerConnection
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.util.Base64
import android.util.Log
import android.util.Size
import androidx.activity.result.ActivityResult
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import com.zerotier.sockets.ZeroTierNative
import com.zerotier.sockets.ZeroTierNode
import com.zerotier.sockets.ZeroTierSocket
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.UUID

@InvokeArg
class ThumbnailArgs {
    lateinit var uri: String
    var size: Int = 256
}

@InvokeArg
class ExportArgs {
    lateinit var uri: String
    var name: String? = null
}

@InvokeArg
class ProgressArgs {
    var title: String = "ZeroSMB"
    var text: String = ""
    var current: Int = 0
    var total: Int = 0
    var done: Boolean = false
}

@InvokeArg
class OpenZeroTierArgs {
    lateinit var networkId: String
}

@InvokeArg
class TreeArgs {
    lateinit var uri: String
}

@InvokeArg
class ZtJoinArgs {
    lateinit var networkId: String
    var planetPath: String? = null
}

@InvokeArg
class ZtStatusArgs {
    var networkId: String? = null
    var planetPath: String? = null
}

@InvokeArg
class ProxyArgs {
    lateinit var host: String
    var port: Int = 445
    lateinit var networkId: String
    var planetPath: String? = null
}

@InvokeArg
class InstallApkArgs {
    lateinit var path: String
}

@InvokeArg
class OpenFileArgs {
    lateinit var path: String
    var name: String = "file"
    var mime: String = "application/octet-stream"
}

@InvokeArg
class SaveDownloadArgs {
    lateinit var path: String
    lateinit var name: String
    var mime: String? = null
    var relativePath: String? = null
}

@InvokeArg
class SaveGalleryArgs {
    lateinit var path: String
    lateinit var name: String
    var mime: String? = null
    var relativePath: String? = null
    var conflictPolicy: String? = null
}

@TauriPlugin
class ZeroSmbPlugin(private val activity: Activity) : Plugin(activity) {
    private external fun ztsInitSetRoots(data: ByteArray, len: Int): Int

    private val ztLock = Any()
    private var ztNode: ZeroTierNode? = null
    private var ztNetworkId: Long = 0L
    private var ztNetworkHex: String = ""
    private var ztPlanetPath: String? = null
    private var proxyServer: ServerSocket? = null
    private var proxyTargetHost: String = ""
    private var proxyTargetPort: Int = 0
    private var proxyPort: Int = 0
    private val proxyClients = mutableListOf<Socket>()
    private val proxyZtSockets = mutableListOf<ZeroTierSocket>()

    private val mediaPermissions: Array<String>
        get() = when {
            Build.VERSION.SDK_INT >= 34 -> arrayOf(
                Manifest.permission.READ_MEDIA_IMAGES,
                Manifest.permission.READ_MEDIA_VIDEO,
                Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED
            )
            Build.VERSION.SDK_INT >= 33 -> arrayOf(
                Manifest.permission.READ_MEDIA_IMAGES,
                Manifest.permission.READ_MEDIA_VIDEO
            )
            else -> arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
        }

    private fun hasMediaPermission(): Boolean {
        if (Build.VERSION.SDK_INT >= 34 &&
            ActivityCompat.checkSelfPermission(
                activity,
                Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED
            ) == PackageManager.PERMISSION_GRANTED
        ) {
            return true
        }
        if (Build.VERSION.SDK_INT >= 33) {
            return ActivityCompat.checkSelfPermission(
                activity,
                Manifest.permission.READ_MEDIA_IMAGES
            ) == PackageManager.PERMISSION_GRANTED ||
                ActivityCompat.checkSelfPermission(
                    activity,
                    Manifest.permission.READ_MEDIA_VIDEO
                ) == PackageManager.PERMISSION_GRANTED
        }
        return ActivityCompat.checkSelfPermission(
            activity,
            Manifest.permission.READ_EXTERNAL_STORAGE
        ) == PackageManager.PERMISSION_GRANTED
    }

    private fun cacheDir(): File =
        File(activity.cacheDir, "zerosmb_export").also { it.mkdirs() }

    private fun safeName(name: String?): String {
        val cleaned = (name ?: "media").replace(Regex("[^A-Za-z0-9._-]"), "_").take(100)
        return if (cleaned.isBlank()) "media" else cleaned
    }

    private fun exportUri(uri: Uri, name: String?): File {
        val file = File(cacheDir(), "${UUID.randomUUID()}-${safeName(name)}")
        activity.contentResolver.openInputStream(uri)?.use { input ->
            file.outputStream().use { output -> input.copyTo(output) }
        } ?: throw IllegalStateException("Не удалось открыть файл")
        return file
    }

    private fun listMediaInternal(): JSArray {
        val items = JSArray()
        val projection = mutableListOf(
            MediaStore.Files.FileColumns._ID,
            MediaStore.Files.FileColumns.DISPLAY_NAME,
            MediaStore.Files.FileColumns.MIME_TYPE,
            MediaStore.Files.FileColumns.SIZE,
            MediaStore.Files.FileColumns.DATE_MODIFIED,
            MediaStore.MediaColumns.DATE_ADDED,
            MediaStore.Images.ImageColumns.DATE_TAKEN,
            MediaStore.Images.ImageColumns.BUCKET_DISPLAY_NAME
        )
        projection.add(MediaStore.MediaColumns.DATA)
        if (Build.VERSION.SDK_INT >= 29) {
            projection.add(MediaStore.MediaColumns.RELATIVE_PATH)
        }
        val collection = if (Build.VERSION.SDK_INT >= 29) {
            MediaStore.Files.getContentUri(MediaStore.VOLUME_EXTERNAL)
        } else {
            MediaStore.Files.getContentUri("external")
        }
        val selection = "${MediaStore.Files.FileColumns.MEDIA_TYPE} IN (?,?)"
        val args = arrayOf(
            MediaStore.Files.FileColumns.MEDIA_TYPE_IMAGE.toString(),
            MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO.toString()
        )
        activity.contentResolver.query(
            collection,
            projection.toTypedArray(),
            selection,
            args,
            "${MediaStore.Files.FileColumns.DATE_MODIFIED} DESC"
        )?.use { cursor ->
            val idCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns._ID)
            val nameCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns.DISPLAY_NAME)
            val mimeCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns.MIME_TYPE)
            val sizeCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns.SIZE)
            val dateCol = cursor.getColumnIndexOrThrow(MediaStore.Files.FileColumns.DATE_MODIFIED)
            val addedCol = cursor.getColumnIndex(MediaStore.MediaColumns.DATE_ADDED)
            val takenCol = cursor.getColumnIndex(MediaStore.Images.ImageColumns.DATE_TAKEN)
            val bucketCol = cursor.getColumnIndexOrThrow(
                MediaStore.Images.ImageColumns.BUCKET_DISPLAY_NAME
            )
            val relativeCol = if (Build.VERSION.SDK_INT >= 29) {
                cursor.getColumnIndex(MediaStore.MediaColumns.RELATIVE_PATH)
            } else {
                -1
            }
            val dataCol = cursor.getColumnIndex(MediaStore.MediaColumns.DATA)
            while (cursor.moveToNext()) {
                val id = cursor.getLong(idCol)
                val mime = cursor.getString(mimeCol) ?: "application/octet-stream"
                if (!mime.startsWith("image/") && !mime.startsWith("video/")) continue
                val uri = ContentUris.withAppendedId(collection, id)
                val item = JSObject()
                item.put("uri", uri.toString())
                item.put("name", cursor.getString(nameCol) ?: "media")
                item.put("mime", mime)
                item.put("size", if (cursor.isNull(sizeCol)) 0L else cursor.getLong(sizeCol))
                val modified = if (cursor.isNull(dateCol)) 0L else cursor.getLong(dateCol)
                val added = if (addedCol < 0 || cursor.isNull(addedCol)) 0L else cursor.getLong(addedCol)
                val taken = if (takenCol < 0 || cursor.isNull(takenCol)) 0L else cursor.getLong(takenCol)
                val created = when {
                    taken > 0 -> taken / 1000
                    added > 0 -> added
                    else -> modified
                }
                item.put("dateModified", modified)
                item.put("dateCreated", created)
                var relative = if (relativeCol < 0 || cursor.isNull(relativeCol)) {
                    ""
                } else {
                    cursor.getString(relativeCol) ?: ""
                }
                var fromData = false
                if (relative.isBlank() && dataCol >= 0 && !cursor.isNull(dataCol)) {
                    relative = cursor.getString(dataCol) ?: ""
                    fromData = true
                }
                relative = relative.replace('\\', '/').trim('/')
                val root = listOf(
                    "storage/emulated/0/",
                    "sdcard/",
                    "mnt/sdcard/",
                    "/storage/emulated/0/",
                    "/sdcard/",
                    "/mnt/sdcard/",
                ).firstOrNull { relative.startsWith(it) }
                if (root != null) relative = relative.removePrefix(root)
                if (fromData || Build.VERSION.SDK_INT < 29) {
                    relative = relative.substringBeforeLast('/', "")
                }
                item.put("bucket", cursor.getString(bucketCol) ?: "")
                item.put("relativePath", relative)
                items.put(item)
            }
        }
        return items
    }

    @Command
    fun listMedia(invoke: Invoke) {
        if (!hasMediaPermission()) {
            handle!!.requestPermissions(invoke, mediaPermissions, "onMediaPermission")
            return
        }
        Thread {
            try {
                val ret = JSObject()
                ret.put("items", listMediaInternal())
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось прочитать галерею")
            }
        }.start()
    }

    @PermissionCallback
    fun onMediaPermission(invoke: Invoke) {
        if (!hasMediaPermission()) {
            invoke.reject("Нет разрешения на доступ к фото и видео")
            return
        }
        listMedia(invoke)
    }

    private fun hasLocalNetworkPermission(): Boolean {
        if (Build.VERSION.SDK_INT < 37) return true
        return ActivityCompat.checkSelfPermission(
            activity,
            Manifest.permission.ACCESS_LOCAL_NETWORK
        ) == PackageManager.PERMISSION_GRANTED
    }

    @Command
    fun ensureLocalNetworkPermission(invoke: Invoke) {
        if (hasLocalNetworkPermission()) {
            val ret = JSObject()
            ret.put("granted", true)
            invoke.resolve(ret)
            return
        }
        handle!!.requestPermissions(
            invoke,
            arrayOf(Manifest.permission.ACCESS_LOCAL_NETWORK),
            "onLocalNetworkPermission"
        )
    }

    @PermissionCallback
    fun onLocalNetworkPermission(invoke: Invoke) {
        if (!hasLocalNetworkPermission()) {
            invoke.reject(
                "Разрешите доступ к локальной сети: Настройки → Приложения → ZeroSMB → Разрешения → Рядом находящиеся устройства / Локальная сеть."
            )
            return
        }
        val ret = JSObject()
        ret.put("granted", true)
        invoke.resolve(ret)
    }
    @Command
    fun getThumbnail(invoke: Invoke) {
        val args = invoke.parseArgs(ThumbnailArgs::class.java)
        val size = args.size.coerceIn(64, 1024)
        Thread {
            try {
                val uri = Uri.parse(args.uri)
                val bitmap: Bitmap? = if (Build.VERSION.SDK_INT >= 29) {
                    activity.contentResolver.loadThumbnail(uri, Size(size, size), null)
                } else {
                    activity.contentResolver.openInputStream(uri)?.use {
                        BitmapFactory.decodeStream(it)
                    }
                }
                if (bitmap == null) {
                    invoke.reject("Миниатюра недоступна")
                    return@Thread
                }
                val output = ByteArrayOutputStream()
                bitmap.compress(Bitmap.CompressFormat.JPEG, 82, output)
                val dataUrl = "data:image/jpeg;base64," +
                    Base64.encodeToString(output.toByteArray(), Base64.NO_WRAP)
                val ret = JSObject()
                ret.put("dataUrl", dataUrl)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось создать миниатюру")
            }
        }.start()
    }

    @Command
    fun exportMedia(invoke: Invoke) {
        val args = invoke.parseArgs(ExportArgs::class.java)
        Thread {
            try {
                val file = exportUri(Uri.parse(args.uri), args.name)
                val ret = JSObject()
                ret.put("path", file.absolutePath)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось экспортировать файл")
            }
        }.start()
    }

    @Command
    fun requestNotificationPermission(invoke: Invoke) {
        if (
            Build.VERSION.SDK_INT >= 33 &&
            activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(
                activity,
                arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                701,
            )
        }
        invoke.resolve()
    }

    @Command
    fun transferProgress(invoke: Invoke) {
        val args = invoke.parseArgs(ProgressArgs::class.java)
        if (
            Build.VERSION.SDK_INT >= 33 &&
            activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(
                activity,
                arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                701,
            )
        }
        try {
            val context = activity.applicationContext
            val intent = Intent(context, ZeroSmbProgressService::class.java).apply {
                putExtra("title", args.title)
                putExtra("text", args.text)
                putExtra("current", args.current)
                putExtra("total", args.total)
                putExtra("done", args.done)
            }
            ContextCompat.startForegroundService(context, intent)
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject(error.message ?: "Не удалось показать уведомление")
        }
    }

    @Command
    fun pickDirectory(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "onTreePicked")
    }

    @ActivityCallback
    fun onTreePicked(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            val ret = JSObject()
            ret.put("cancelled", true)
            invoke.resolve(ret)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            invoke.reject("Папка не выбрана")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            )
        } catch (_: Exception) {
        }
        val ret = JSObject()
        ret.put("uri", uri.toString())
        ret.put(
            "name",
            DocumentsContract.getTreeDocumentId(uri).substringAfterLast(':')
                .ifBlank { "Android folder" }
        )
        invoke.resolve(ret)
    }

    private fun walkTree(treeUri: Uri, parentDocId: String, prefix: String, out: JSArray) {
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED
        )
        activity.contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
            val idCol = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameCol = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeCol = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
            val sizeCol = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_SIZE)
            val dateCol = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
            while (cursor.moveToNext()) {
                val docId = cursor.getString(idCol) ?: continue
                val name = cursor.getString(nameCol) ?: continue
                val mime = cursor.getString(mimeCol) ?: ""
                val isDir = mime == DocumentsContract.Document.MIME_TYPE_DIR
                val rel = if (prefix.isEmpty()) name else "$prefix/$name"
                val childUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, docId)
                val entry = JSObject()
                entry.put("relativePath", rel)
                entry.put("uri", childUri.toString())
                entry.put("name", name)
                entry.put("size", if (cursor.isNull(sizeCol)) 0L else cursor.getLong(sizeCol))
                entry.put(
                    "lastModified",
                    if (cursor.isNull(dateCol)) 0L else cursor.getLong(dateCol)
                )
                entry.put("isDirectory", isDir)
                out.put(entry)
                if (isDir) walkTree(treeUri, docId, rel, out)
            }
        }
    }

    @Command
    fun listTree(invoke: Invoke) {
        val args = invoke.parseArgs(TreeArgs::class.java)
        Thread {
            try {
                val treeUri = Uri.parse(args.uri)
                val rootId = DocumentsContract.getTreeDocumentId(treeUri)
                val entries = JSArray()
                walkTree(treeUri, rootId, "", entries)
                val ret = JSObject()
                ret.put("entries", entries)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось прочитать папку")
            }
        }.start()
    }

    @Command
    fun exportTreeFile(invoke: Invoke) {
        val args = invoke.parseArgs(ExportArgs::class.java)
        Thread {
            try {
                val file = exportUri(Uri.parse(args.uri), args.name)
                val ret = JSObject()
                ret.put("path", file.absolutePath)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось экспортировать файл")
            }
        }.start()
    }

    private fun parseNetworkId(raw: String): Long {
        val cleaned = raw
            .trim()
            .removePrefix("0x")
            .removePrefix("0X")
            .replace(Regex("[\\s:.-]"), "")
        if (!cleaned.matches(Regex("[0-9a-fA-F]{10,16}"))) {
            throw IllegalArgumentException(
                "Некорректный ZeroTier Network ID: нужно 16 hex-символов, получено ${cleaned.length}"
            )
        }
        return java.lang.Long.parseUnsignedLong(cleaned, 16)
    }

    private fun ztFailure(error: Throwable): String {
        val detail = buildString {
            append(error.javaClass.name)
            error.message?.let { append(": ").append(it) }
        }
        try {
            File(activity.filesDir, "zt_last_error.txt").writeText(
                "${System.currentTimeMillis()}\n$detail\n\n${error.stackTraceToString()}"
            )
        } catch (_: Throwable) {
        }
        Log.e("ZeroSMB", "ZeroTier failure: $detail", error)
        return detail
    }

    private fun hexNetworkId(id: Long): String =
        java.lang.Long.toHexString(id).padStart(16, '0')

    private fun ensureNodeLocked(planetPath: String?): ZeroTierNode {
        val wantedPlanet = planetPath?.trim()?.takeIf { it.isNotEmpty() }
        if (ztNode != null && ztPlanetPath != wantedPlanet) {
            stopProxyLocked()
            ztNode?.stop()
            ztNode = null
            ztNetworkId = 0L
            ztNetworkHex = ""
        }
        ztNode?.let { node ->
            if (node.isOnline) return node
            stopProxyLocked()
            node.stop()
            ztNode = null
            ztNetworkId = 0L
            ztNetworkHex = ""
        }
        val dir = File(activity.filesDir, "zerotier")
        dir.mkdirs()
        try {
            System.loadLibrary("zt")
            System.loadLibrary("ztShim")
        } catch (error: Throwable) {
            throw IllegalStateException("Не удалось загрузить libzt: ${error.message}", error)
        }
        val node = ZeroTierNode()
        val initResult = node.initFromStorage(dir.absolutePath)
        if (initResult < 0) {
            throw IllegalStateException("ZeroTier: initFromStorage ($initResult)")
        }
        if (wantedPlanet != null) {
            val planetFile = File(wantedPlanet)
            if (!planetFile.isFile) {
                throw IllegalStateException("ZeroTier: файл planet не найден")
            }
            val planet = planetFile.readBytes()
            if (planet.isEmpty() || planet.size > 4096) {
                throw IllegalStateException("ZeroTier: некорректный planet (размер ${planet.size})")
            }
            val rootsResult = ztsInitSetRoots(planet, planet.size)
            if (rootsResult != 0) {
                throw IllegalStateException("ZeroTier: set_roots ($rootsResult)")
            }
            ZeroTierNative.zts_init_allow_roots_cache(0)
        }
        ztPlanetPath = wantedPlanet
        node.initSetPort(9994)
        val startResult = node.start()
        if (startResult < 0) {
            throw IllegalStateException("ZeroTier: node.start ($startResult)")
        }
        ztNode = node
        val deadline = System.currentTimeMillis() + 20000
        while (!node.isOnline && System.currentTimeMillis() < deadline) {
            Thread.sleep(100)
        }
        return node
    }

    private fun ensureNetworkLocked(
        networkHex: String,
        requireReady: Boolean,
        planetPath: String?,
    ): ZeroTierNode {
        val networkId = parseNetworkId(networkHex)
        val node = ensureNodeLocked(planetPath)
        if (!node.isOnline) {
            throw IllegalStateException(
                "ZeroTier: нет соединения с корневыми серверами. Проверьте интернет."
            )
        }
        if (ztNetworkId == networkId && !node.isNetworkTransportReady(networkId)) {
            node.leave(networkId)
            ztNetworkId = 0L
            ztNetworkHex = ""
        }
        if (ztNetworkId != networkId) {
            val result = node.join(networkId)
            if (result < 0) {
                throw IllegalStateException("ZeroTier: не удалось войти в сеть ($result)")
            }
            ztNetworkId = networkId
            ztNetworkHex = hexNetworkId(networkId)
        }
        val deadline = System.currentTimeMillis() + 25000
        while (!node.isNetworkTransportReady(networkId) && System.currentTimeMillis() < deadline) {
            Thread.sleep(100)
        }
        if (requireReady && !node.isNetworkTransportReady(networkId)) {
            throw IllegalStateException(
                "ZeroTier: сеть ещё не готова. Авторизуйте устройство в ZeroTier Central и повторите."
            )
        }
        return node
    }

    private fun ztStatusObject(): JSObject {
        val node = ztNode
        val online = node?.isOnline == true
        val ready = node != null && ztNetworkId != 0L &&
            node.isNetworkTransportReady(ztNetworkId)
        var ipv4 = ""
        if (ready) {
            try {
                ipv4 = node!!.getIPv4Address(ztNetworkId)?.hostAddress ?: ""
            } catch (_: Exception) {
            }
        }
        val nodeId = try {
            node?.let { java.lang.Long.toHexString(it.id).padStart(10, '0') } ?: ""
        } catch (_: Exception) {
            ""
        }
        val ret = JSObject()
        ret.put("started", node != null)
        ret.put("online", online)
        ret.put("networkReady", ready)
        ret.put("nodeId", nodeId)
        ret.put("ipv4", ipv4)
        ret.put("networkId", ztNetworkHex)
        ret.put(
            "message",
            when {
                ready -> "ZeroTier подключён (встроенный libzt). IP: $ipv4"
                online -> "ZeroTier онлайн, ожидание сети или авторизации в ZeroTier Central"
                node != null -> "ZeroTier запускается…"
                else -> "ZeroTier не запущен"
            }
        )
        return ret
    }

    @Command
    fun ztJoin(invoke: Invoke) {
        val args = invoke.parseArgs(ZtJoinArgs::class.java)
        Thread {
            try {
                synchronized(ztLock) {
                    ensureNetworkLocked(args.networkId, false, args.planetPath)
                    invoke.resolve(ztStatusObject())
                }
            } catch (error: Throwable) {
                invoke.reject(ztFailure(error))
            }
        }.start()
    }

    @Command
    fun ztStatus(invoke: Invoke) {
        val args = invoke.parseArgs(ZtStatusArgs::class.java)
        Thread {
            try {
                synchronized(ztLock) {
                    val networkId = args.networkId?.trim().orEmpty()
                    if (networkId.isNotEmpty() && ztNode == null) {
                        ensureNetworkLocked(networkId, false, args.planetPath)
                    }
                    invoke.resolve(ztStatusObject())
                }
            } catch (error: Throwable) {
                invoke.reject(ztFailure(error))
            }
        }.start()
    }

    @Command
    fun ztLeave(invoke: Invoke) {
        Thread {
            try {
                synchronized(ztLock) {
                    val node = ztNode
                    if (node != null && ztNetworkId != 0L) {
                        node.leave(ztNetworkId)
                    }
                    stopProxyLocked()
                    ztNetworkId = 0L
                    ztNetworkHex = ""
                    invoke.resolve(ztStatusObject())
                }
            } catch (error: Throwable) {
                invoke.reject(ztFailure(error))
            }
        }.start()
    }

    @Command
    fun ztStop(invoke: Invoke) {
        Thread {
            try {
                synchronized(ztLock) {
                    stopProxyLocked()
                    ztNode?.stop()
                    ztNode = null
                    ztNetworkId = 0L
                    ztNetworkHex = ""
                    invoke.resolve(ztStatusObject())
                }
            } catch (error: Throwable) {
                invoke.reject(ztFailure(error))
            }
        }.start()
    }

    @Command
    fun startSmbProxy(invoke: Invoke) {
        val args = invoke.parseArgs(ProxyArgs::class.java)
        Thread {
            try {
                val port = synchronized(ztLock) {
                    ensureNetworkLocked(args.networkId, true, args.planetPath)
                    startProxyLocked(args.host, args.port)
                }
                val ret = JSObject()
                ret.put("port", port)
                invoke.resolve(ret)
            } catch (error: Throwable) {
                invoke.reject(ztFailure(error))
            }
        }.start()
    }

    @Command
    fun stopSmbProxy(invoke: Invoke) {
        synchronized(ztLock) {
            stopProxyLocked()
        }
        invoke.resolve()
    }

    private fun startProxyLocked(host: String, port: Int): Int {
        if (proxyServer != null && proxyTargetHost == host && proxyTargetPort == port) {
            return proxyPort
        }
        stopProxyLocked()
        val server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
        proxyServer = server
        proxyPort = server.localPort
        proxyTargetHost = host
        proxyTargetPort = port
        Thread {
            while (!server.isClosed) {
                try {
                    val client = server.accept()
                    synchronized(ztLock) {
                        proxyClients.add(client)
                    }
                    Thread {
                        handleProxyClient(client, host, port)
                    }.start()
                } catch (_: Exception) {
                    if (server.isClosed) break
                }
            }
        }.start()
        return proxyPort
    }

    private fun handleProxyClient(client: Socket, host: String, port: Int) {
        var ztSocket: ZeroTierSocket? = null
        try {
            val target = ZeroTierSocket(host, port)
            ztSocket = target
            synchronized(ztLock) {
                proxyZtSockets.add(target)
            }
            Thread {
                try {
                    pipe(client.getInputStream(), target.getOutputStream())
                } catch (_: Exception) {
                } finally {
                    closeQuietly(client)
                    closeQuietly(target)
                }
            }.start()
            pipe(target.getInputStream(), client.getOutputStream())
        } catch (_: Exception) {
        } finally {
            closeQuietly(client)
            ztSocket?.let {
                closeQuietly(it)
                synchronized(ztLock) {
                    proxyZtSockets.remove(it)
                }
            }
            synchronized(ztLock) {
                proxyClients.remove(client)
            }
        }
    }

    private fun pipe(input: InputStream, output: OutputStream) {
        val buffer = ByteArray(32768)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            output.write(buffer, 0, read)
            output.flush()
        }
    }

    private fun closeQuietly(closeable: Any?) {
        try {
            when (closeable) {
                is Socket -> closeable.close()
                is ZeroTierSocket -> closeable.close()
                is ServerSocket -> closeable.close()
            }
        } catch (_: Exception) {
        }
    }

    private fun stopProxyLocked() {
        closeQuietly(proxyServer)
        proxyServer = null
        proxyPort = 0
        proxyTargetHost = ""
        proxyTargetPort = 0
        for (client in proxyClients) closeQuietly(client)
        proxyClients.clear()
        for (socket in proxyZtSockets) closeQuietly(socket)
        proxyZtSockets.clear()
    }

    @Command
    fun pickFiles(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        startActivityForResult(invoke, intent, "onFilesPicked")
    }

    @ActivityCallback
    fun onFilesPicked(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            invoke.reject("Файлы не выбраны")
            return
        }
        val uris = mutableListOf<Uri>()
        val clip = result.data?.clipData
        if (clip != null) {
            for (index in 0 until clip.itemCount) {
                clip.getItemAt(index)?.uri?.let { uris.add(it) }
            }
        } else {
            result.data?.data?.let { uris.add(it) }
        }
        if (uris.isEmpty()) {
            invoke.reject("Файлы не выбраны")
            return
        }
        Thread {
            try {
                val files = JSArray()
                for (uri in uris) {
                    val name = displayName(uri)
                    val file = exportUri(uri, name)
                    val item = JSObject()
                    item.put("path", file.absolutePath)
                    item.put("name", name)
                    files.put(item)
                }
                val ret = JSObject()
                ret.put("files", files)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось прочитать файлы")
            }
        }.start()
    }

    @Command
    fun pickUploadFolder(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "onUploadFolderPicked")
    }

    @ActivityCallback
    fun onUploadFolderPicked(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            val ret = JSObject()
            ret.put("cancelled", true)
            invoke.resolve(ret)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            invoke.reject("Папка не выбрана")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            )
        } catch (_: Exception) {
        }
        Thread {
            try {
                val rootId = DocumentsContract.getTreeDocumentId(uri)
                val rootName = rootId.substringAfterLast(':').ifBlank { "Папка" }
                val files = JSArray()
                exportUploadTree(uri, rootId, "", rootName, files)
                val ret = JSObject()
                ret.put("files", files)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось прочитать папку")
            }
        }.start()
    }

    private fun exportUploadTree(
        treeUri: Uri,
        parentDocId: String,
        prefix: String,
        rootName: String,
        out: JSArray
    ) {
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        activity.contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
            val idCol = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameCol = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeCol = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
            while (cursor.moveToNext()) {
                val docId = cursor.getString(idCol) ?: continue
                val name = cursor.getString(nameCol) ?: continue
                val mime = cursor.getString(mimeCol) ?: ""
                val relative = if (prefix.isEmpty()) name else "$prefix/$name"
                if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                    exportUploadTree(treeUri, docId, relative, rootName, out)
                } else {
                    val childUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, docId)
                    val file = exportUri(childUri, name)
                    val item = JSObject()
                    item.put("path", file.absolutePath)
                    item.put("name", name)
                    item.put("relativePath", "$rootName/$relative")
                    out.put(item)
                }
            }
        }
    }

    private fun displayName(uri: Uri): String {
        try {
            activity.contentResolver.query(
                uri,
                arrayOf(OpenableColumns.DISPLAY_NAME),
                null,
                null,
                null
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    return cursor.getString(0) ?: "file"
                }
            }
        } catch (_: Exception) {
        }
        return uri.lastPathSegment ?: "file"
    }

    @Command
    fun saveToDownloads(invoke: Invoke) {
        val args = invoke.parseArgs(SaveDownloadArgs::class.java)
        Thread {
            try {
                val source = File(args.path)
                if (!source.isFile) {
                    throw IllegalStateException("Скачанный файл не найден")
                }
                val mime = args.mime ?: "application/octet-stream"
                val displayName = args.name.replace(Regex("[\\\\/]"), "_").ifBlank { "file" }
                val subdir = (args.relativePath ?: "")
                    .replace('\\', '/')
                    .split('/')
                    .filter { it.isNotBlank() && it != "." && it != ".." }
                    .joinToString("/")
                val relativeDir = if (subdir.isEmpty()) "ZeroSMB" else "ZeroSMB/$subdir"
                val savedPath: String
                if (Build.VERSION.SDK_INT >= 29) {
                    val values = ContentValues().apply {
                        put(MediaStore.MediaColumns.DISPLAY_NAME, displayName)
                        put(MediaStore.MediaColumns.MIME_TYPE, mime)
                        put(
                            MediaStore.MediaColumns.RELATIVE_PATH,
                            Environment.DIRECTORY_DOWNLOADS + "/" + relativeDir
                        )
                        put(MediaStore.MediaColumns.IS_PENDING, 1)
                    }
                    val resolver = activity.contentResolver
                    val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                        ?: throw IllegalStateException("Не удалось создать файл в Downloads")
                    resolver.openOutputStream(uri)?.use { output ->
                        source.inputStream().use { input -> input.copyTo(output) }
                    } ?: throw IllegalStateException("Не удалось записать файл в Downloads")
                    val done = ContentValues()
                    done.put(MediaStore.MediaColumns.IS_PENDING, 0)
                    resolver.update(uri, done, null, null)
                    savedPath = "Download/$relativeDir/$displayName"
                } else {
                    val directory = File(
                        activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS),
                        relativeDir
                    )
                    directory.mkdirs()
                    val destination = File(directory, displayName)
                    source.copyTo(destination, true)
                    savedPath = destination.absolutePath
                }
                val ret = JSObject()
                ret.put("path", savedPath)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось сохранить файл")
            }
        }.start()
    }
    private fun sanitizeRelative(value: String?): String =
        (value ?: "")
            .replace('\\', '/')
            .split('/')
            .filter { it.isNotBlank() && it != "." && it != ".." }
            .joinToString("/")

    private fun mediaCollection(mime: String): Uri =
        if (mime.startsWith("video/")) {
            MediaStore.Video.Media.EXTERNAL_CONTENT_URI
        } else {
            MediaStore.Images.Media.EXTERNAL_CONTENT_URI
        }



    private fun mediaRelativePath(mime: String, folder: String): String {
        val base = if (mime.startsWith("video/")) {
            Environment.DIRECTORY_MOVIES
        } else {
            Environment.DIRECTORY_PICTURES
        }
        val parts = folder.split('/').filter { it.isNotBlank() }
        val clean = if (parts.isNotEmpty() && parts.first().equals(base, ignoreCase = true)) {
            parts.drop(1)
        } else {
            parts
        }
        return if (clean.isEmpty()) base else "$base/${clean.joinToString("/")}"
    }

    private fun mediaNameExists(collection: Uri, relativePath: String, name: String): Boolean {
        if (Build.VERSION.SDK_INT >= 29) {
            val resolver = activity.contentResolver
            val selection =
                "${MediaStore.MediaColumns.RELATIVE_PATH}=? AND ${MediaStore.MediaColumns.DISPLAY_NAME}=?"
            val path = if (relativePath.endsWith("/")) relativePath else "$relativePath/"
            resolver.query(
                collection,
                arrayOf(MediaStore.MediaColumns._ID),
                selection,
                arrayOf(path, name),
                null
            )?.use { cursor -> return cursor.moveToFirst() }
            return false
        }
        val directory = File(Environment.getExternalStorageDirectory(), relativePath)
        return File(directory, name).isFile()
    }

    private fun uniqueMediaName(
        collection: Uri,
        relativePath: String,
        name: String
    ): String {
        val dot = name.lastIndexOf('.')
        val stem = if (dot > 0) name.substring(0, dot) else name
        val extension = if (dot > 0) name.substring(dot) else ""
        for (index in 1..10_000) {
            val candidate = "$stem ($index)$extension"
            if (!mediaNameExists(collection, relativePath, candidate)) return candidate
        }
        return name
    }

    @Command
    fun saveToGallery(invoke: Invoke) {
        val args = invoke.parseArgs(SaveGalleryArgs::class.java)
        Thread {
            try {
                val source = File(args.path)
                if (!source.isFile) {
                    throw IllegalStateException("Скачанный файл не найден")
                }
                val mime = args.mime ?: "application/octet-stream"
                val originalName = args.name.replace(Regex("[\\\\/]"), "_").ifBlank { "file" }
                val folder = sanitizeRelative(args.relativePath)
                val relativePath = mediaRelativePath(mime, folder)
                val collection = mediaCollection(mime)
                val policy = args.conflictPolicy ?: "skip"
                var displayName = originalName
                if (mediaNameExists(collection, relativePath, displayName)) {
                    if (policy != "rename") {
                        val ret = JSObject()
                        ret.put("path", "$relativePath/$displayName")
                        ret.put("skipped", true)
                        invoke.resolve(ret)
                        return@Thread
                    }
                    displayName = uniqueMediaName(collection, relativePath, displayName)
                }
                val savedPath: String
                if (Build.VERSION.SDK_INT >= 29) {
                    val values = ContentValues().apply {
                        put(MediaStore.MediaColumns.DISPLAY_NAME, displayName)
                        put(MediaStore.MediaColumns.MIME_TYPE, mime)
                        put(MediaStore.MediaColumns.RELATIVE_PATH, relativePath)
                        put(MediaStore.MediaColumns.DATE_MODIFIED, source.lastModified() / 1000)
                        put(MediaStore.MediaColumns.IS_PENDING, 1)
                    }
                    val resolver = activity.contentResolver
                    val uri = resolver.insert(collection, values)
                        ?: throw IllegalStateException("Не удалось создать файл в галерее")
                    resolver.openOutputStream(uri)?.use { output ->
                        source.inputStream().use { input -> input.copyTo(output) }
                    } ?: throw IllegalStateException("Не удалось записать файл в галерею")
                    val done = ContentValues()
                    done.put(MediaStore.MediaColumns.IS_PENDING, 0)
                    resolver.update(uri, done, null, null)
                    savedPath = "$relativePath/$displayName"
                } else {
                    var directory = File(
                        Environment.getExternalStorageDirectory(),
                        relativePath
                    )
                    if (!directory.exists() && !directory.mkdirs()) {
                        directory = File(activity.getExternalFilesDir(relativePath), "")
                        directory.mkdirs()
                    }
                    val destination = File(directory, displayName)
                    source.copyTo(destination, true)
                    MediaScannerConnection.scanFile(
                        activity,
                        arrayOf(destination.absolutePath),
                        arrayOf(mime),
                        null
                    )
                    savedPath = destination.absolutePath
                }
                val ret = JSObject()
                ret.put("path", savedPath)
                ret.put("skipped", false)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось сохранить файл в галерею")
            }
        }.start()
    }
    @Command
    fun pickPlanet(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "onPlanetPicked")
    }

    @ActivityCallback
    fun onPlanetPicked(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            val ret = JSObject()
            ret.put("cancelled", true)
            invoke.resolve(ret)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            invoke.reject("Файл planet не выбран")
            return
        }
        Thread {
            try {
                val file = exportUri(uri, "planet")
                val ret = JSObject()
                ret.put("path", file.absolutePath)
                invoke.resolve(ret)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "Не удалось прочитать planet")
            }
        }.start()
    }
    @Command
    fun installApk(invoke: Invoke) {
        val args = invoke.parseArgs(InstallApkArgs::class.java)
        try {
            val file = File(args.path)
            if (!file.exists()) {
                invoke.reject("APK не найден")
                return
            }
            val uri = FileProvider.getUriForFile(
                activity,
                activity.packageName + ".fileprovider",
                file
            )
            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, "application/vnd.android.package-archive")
                addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_ACTIVITY_NEW_TASK
                )
            }
            activity.startActivity(intent)
            val ret = JSObject()
            ret.put("started", true)
            invoke.resolve(ret)
        } catch (error: Exception) {
            invoke.reject(error.message ?: "Не удалось запустить установку APK")
        }
    }

    @Command
    fun openFile(invoke: Invoke) {
        val args = invoke.parseArgs(OpenFileArgs::class.java)
        try {
            val file = File(args.path)
            if (!file.exists()) {
                invoke.reject("Файл не найден")
                return
            }
            val uri = FileProvider.getUriForFile(
                activity,
                activity.packageName + ".fileprovider",
                file
            )
            val permissions =
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            val editIntent = Intent(Intent.ACTION_EDIT).apply {
                setDataAndType(uri, args.mime)
                clipData = ClipData.newUri(activity.contentResolver, args.name, uri)
                putExtra(Intent.EXTRA_TITLE, args.name)
                addFlags(permissions or Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            val viewIntent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, args.mime)
                clipData = ClipData.newUri(activity.contentResolver, args.name, uri)
                putExtra(Intent.EXTRA_TITLE, args.name)
                addFlags(permissions or Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            activity.packageManager
                .queryIntentActivities(editIntent, PackageManager.MATCH_DEFAULT_ONLY)
                .forEach { info ->
                    activity.grantUriPermission(
                        info.activityInfo.packageName,
                        uri,
                        Intent.FLAG_GRANT_READ_URI_PERMISSION or
                            Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                    )
                }
            try {
                activity.startActivity(editIntent)
            } catch (_: ActivityNotFoundException) {
                activity.startActivity(viewIntent)
            }
            val ret = JSObject()
            ret.put("started", true)
            invoke.resolve(ret)
        } catch (error: Exception) {
            invoke.reject(error.message ?: "Не удалось открыть файл")
        }
    }

    @Command
    fun openZeroTier(invoke: Invoke) {
        val args = invoke.parseArgs(OpenZeroTierArgs::class.java)
        if (args.networkId.isNotBlank()) {
            try {
                val clipboard = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                clipboard.setPrimaryClip(
                    ClipData.newPlainText("ZeroTier network", args.networkId)
                )
            } catch (_: Exception) {
            }
        }
        var installed = false
        try {
            val launch = activity.packageManager.getLaunchIntentForPackage("com.zerotier.one")
            if (launch != null) {
                installed = true
                activity.startActivity(launch)
            }
        } catch (_: Exception) {
        }
        if (!installed) {
            try {
                activity.startActivity(
                    Intent(
                        Intent.ACTION_VIEW,
                        Uri.parse(
                            "https://play.google.com/store/apps/details?id=com.zerotier.one"
                        )
                    )
                )
            } catch (_: Exception) {
            }
        }
        val ret = JSObject()
        ret.put("installed", installed)
        ret.put("opened", installed)
        ret.put("copied", args.networkId.isNotBlank())
        invoke.resolve(ret)
    }
}
