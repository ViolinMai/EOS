#define PY_SSIZE_T_CLEAN
#include <Python.h>
#include <string.h>
#include <stdio.h>

// Rust callbacks exposed from userspace/src/ffi_python.rs
extern int eos_cb_spawn(const char *app_name);
extern void eos_cb_notify(const char *title, const char *msg);
extern void eos_cb_clipboard_set(const char *text);
extern int eos_cb_clipboard_get(char *out_buf, int max_len);
extern void eos_cb_audit(const char *msg);

static PyObject *py_eos_spawn(PyObject *self, PyObject *args) {
    const char *app = NULL;
    if (!PyArg_ParseTuple(args, "s", &app)) return NULL;
    int res = eos_cb_spawn(app);
    return PyLong_FromLong(res);
}

static PyObject *py_eos_notify(PyObject *self, PyObject *args) {
    const char *title = NULL;
    const char *msg = NULL;
    if (!PyArg_ParseTuple(args, "ss", &title, &msg)) return NULL;
    eos_cb_notify(title, msg);
    Py_RETURN_NONE;
}

static PyObject *py_eos_clipboard_set(PyObject *self, PyObject *args) {
    const char *text = NULL;
    if (!PyArg_ParseTuple(args, "s", &text)) return NULL;
    eos_cb_clipboard_set(text);
    Py_RETURN_NONE;
}

static PyObject *py_eos_clipboard_get(PyObject *self, PyObject *args) {
    char buf[4096];
    int n = eos_cb_clipboard_get(buf, (int)sizeof(buf) - 1);
    if (n < 0) n = 0;
    buf[n] = '\0';
    return PyUnicode_FromStringAndSize(buf, n);
}

static PyObject *py_eos_audit(PyObject *self, PyObject *args) {
    const char *msg = NULL;
    if (!PyArg_ParseTuple(args, "s", &msg)) return NULL;
    eos_cb_audit(msg);
    Py_RETURN_NONE;
}

static PyMethodDef EosMethods[] = {
    {"spawn", py_eos_spawn, METH_VARARGS, "Spawn an isolated EOS application"},
    {"notify", py_eos_notify, METH_VARARGS, "Send a desktop toast notification"},
    {"clipboard_set", py_eos_clipboard_set, METH_VARARGS, "Set kernel clipboard text"},
    {"clipboard_get", py_eos_clipboard_get, METH_NOARGS, "Get kernel clipboard text"},
    {"audit", py_eos_audit, METH_VARARGS, "Emit an [EOS_AUDIT] checkpoint"},
    {NULL, NULL, 0, NULL}
};

static struct PyModuleDef eosmodule = {
    PyModuleDef_HEAD_INIT,
    "eos",
    "Native EOS OS Kernel & Desktop Bindings",
    -1,
    EosMethods
};

PyMODINIT_FUNC PyInit_eos(void) {
    return PyModule_Create(&eosmodule);
}

static void *eos_py_malloc(void *ctx, size_t size) {
    (void)ctx;
    return malloc(size ? size : 1);
}

static void *eos_py_calloc(void *ctx, size_t nelem, size_t elsize) {
    (void)ctx;
    return calloc(nelem ? nelem : 1, elsize ? elsize : 1);
}

static void *eos_py_realloc(void *ctx, void *ptr, size_t new_size) {
    (void)ctx;
    return realloc(ptr, new_size ? new_size : 1);
}

static void eos_py_free(void *ctx, void *ptr) {
    (void)ctx;
    free(ptr);
}

static int g_py_initialized = 0;

int eos_py_init(void) {
    if (g_py_initialized) return 0;

    if (PyImport_AppendInittab("eos", PyInit_eos) == -1) {
        return -1;
    }

    // Use system musl malloc/free via ABI-correct PyMemAllocatorEx wrappers
    PyMemAllocatorEx alloc_hooks;
    alloc_hooks.ctx = NULL;
    alloc_hooks.malloc = eos_py_malloc;
    alloc_hooks.calloc = eos_py_calloc;
    alloc_hooks.realloc = eos_py_realloc;
    alloc_hooks.free = eos_py_free;
    PyMem_SetAllocator(PYMEM_DOMAIN_RAW, &alloc_hooks);
    PyMem_SetAllocator(PYMEM_DOMAIN_MEM, &alloc_hooks);
    PyMem_SetAllocator(PYMEM_DOMAIN_OBJ, &alloc_hooks);

    PyConfig config;
    PyConfig_InitIsolatedConfig(&config);
    config.write_bytecode = 0;
    config.site_import = 0;
    config.install_signal_handlers = 0;
    config.parse_argv = 0;
    config.user_site_directory = 0;
    config.faulthandler = 0;
    config.module_search_paths_set = 1;

    PyWideStringList_Append(&config.module_search_paths, L"/EOS SHARE/python_stdlib");
    PyWideStringList_Append(&config.module_search_paths, L"EOS SHARE/python_stdlib");
    PyWideStringList_Append(&config.module_search_paths, L"/EOS SHARE/python_libs");
    PyWideStringList_Append(&config.module_search_paths, L"EOS SHARE/python_libs");
    PyWideStringList_Append(&config.module_search_paths, L"/r/lib/python3.12/site-packages");

    PyStatus status = Py_InitializeFromConfig(&config);
    if (PyStatus_Exception(status)) {
        char diag[256];
        snprintf(
            diag,
            sizeof(diag),
            "CPYTHON_INIT_ERR: %s (%s)",
            status.err_msg ? status.err_msg : "unknown",
            status.func ? status.func : "Py_InitializeFromConfig"
        );
        eos_cb_audit(diag);
        PyConfig_Clear(&config);
        return -2;
    }
    PyConfig_Clear(&config);

    g_py_initialized = 1;
    return 0;
}

int eos_py_run_code(const char *code, char *out_buf, int out_cap) {
    if (!g_py_initialized && eos_py_init() != 0) {
        return -1;
    }
    if (!code || !out_buf || out_cap <= 1) return -2;

    PyObject *main_mod = PyImport_AddModule("__main__");
    if (!main_mod) return -3;
    PyObject *globals = PyModule_GetDict(main_mod);

    // Capture stdout & stderr into an in-memory buffer
    const char *setup_capture =
        "import sys\n"
        "class _EosCap:\n"
        "    def __init__(self): self.chunks = []\n"
        "    def write(self, s): self.chunks.append(str(s))\n"
        "    def flush(self): pass\n"
        "    def getvalue(self): return ''.join(self.chunks)\n"
        "_eos_cap = _EosCap()\n"
        "_eos_old_out, _eos_old_err = sys.stdout, sys.stderr\n"
        "sys.stdout = _eos_cap\n"
        "sys.stderr = _eos_cap\n";
    PyRun_SimpleString(setup_capture);

    PyObject *res = PyRun_StringFlags(code, Py_file_input, globals, globals, NULL);
    int rc = 0;
    if (!res) {
        PyErr_Print();
        rc = -4;
    } else {
        Py_DECREF(res);
    }

    PyRun_SimpleString("sys.stdout, sys.stderr = _eos_old_out, _eos_old_err\n");

    PyObject *cap_obj = PyDict_GetItemString(globals, "_eos_cap");
    out_buf[0] = '\0';
    if (cap_obj) {
        PyObject *val = PyObject_CallMethod(cap_obj, "getvalue", NULL);
        if (val) {
            Py_ssize_t len = 0;
            const char *utf8 = PyUnicode_AsUTF8AndSize(val, &len);
            if (utf8 && len > 0) {
                int to_copy = (int)len < (out_cap - 1) ? (int)len : (out_cap - 1);
                memcpy(out_buf, utf8, (size_t)to_copy);
                out_buf[to_copy] = '\0';
            }
            Py_DECREF(val);
        }
    }
    return rc;
}

void eos_py_finalize(void) {
    if (g_py_initialized) {
        Py_FinalizeEx();
        g_py_initialized = 0;
    }
}
