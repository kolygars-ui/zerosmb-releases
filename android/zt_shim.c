#include <jni.h>
#include <dlfcn.h>
#include <stddef.h>

typedef int (*zts_set_roots_fn)(const void *, unsigned int);

JNIEXPORT jint JNICALL
Java_com_zerosmb_app_ZeroSmbPlugin_ztsInitSetRoots(
    JNIEnv *env,
    jobject thiz,
    jbyteArray data,
    jint len
) {
    (void)thiz;
    if (data == NULL || len <= 0) {
        return -1;
    }
    void *handle = dlopen("libzt.so", RTLD_NOW | RTLD_GLOBAL);
    zts_set_roots_fn set_roots = handle == NULL
        ? NULL
        : (zts_set_roots_fn)dlsym(handle, "zts_init_set_roots");
    if (set_roots == NULL) {
        set_roots = (zts_set_roots_fn)dlsym(RTLD_DEFAULT, "zts_init_set_roots");
    }
    if (set_roots == NULL) {
        if (handle != NULL) {
            dlclose(handle);
        }
        return -3;
    }
    jbyte *bytes = (*env)->GetByteArrayElements(env, data, NULL);
    if (bytes == NULL) {
        dlclose(handle);
        return -4;
    }
    int result = set_roots(bytes, (unsigned int)len);
    (*env)->ReleaseByteArrayElements(env, data, bytes, JNI_ABORT);
    dlclose(handle);
    return (jint)result;
}
