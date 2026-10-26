"""Disposable foreground tool tree for REQ-023 (Spec §3, SAFE-03)."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def parents():
    if os.name != "nt":
        result = {}
        for entry in Path("/proc").iterdir():
            if entry.name.isdigit():
                try:
                    fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
                    result[int(entry.name)] = int(fields[1])
                except (OSError, ValueError):
                    pass
        return result
    class Entry(ctypes.Structure):
        _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD),
                    ("pid", wintypes.DWORD), ("heap", ctypes.c_size_t),
                    ("module", wintypes.DWORD), ("threads", wintypes.DWORD),
                    ("parent", wintypes.DWORD), ("priority", wintypes.LONG),
                    ("flags", wintypes.DWORD), ("exe", wintypes.WCHAR * 260)]
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    for name in ("Process32FirstW", "Process32NextW"):
        function = getattr(kernel, name)
        function.argtypes = [wintypes.HANDLE, ctypes.POINTER(Entry)]
        function.restype = wintypes.BOOL
    handle = kernel.CreateToolhelp32Snapshot(2, 0)
    if handle == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    result = {}
    try:
        entry = Entry()
        entry.dwSize = ctypes.sizeof(entry)
        valid = kernel.Process32FirstW(handle, ctypes.byref(entry))
        while valid:
            result[entry.pid] = entry.parent
            valid = kernel.Process32NextW(handle, ctypes.byref(entry))
        return result
    finally:
        kernel.CloseHandle(handle)


core = int(sys.argv[1])
child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(90)"])
mapping = parents()
chain = [os.getpid()]
while chain[-1] != core and len(chain) < 64:
    chain.append(mapping[chain[-1]])
assert chain[-1] == core
Path("probe.json.tmp").write_text(json.dumps({"core": core, "pids": [child.pid, *chain[:-1]]}), encoding="utf-8")
Path("probe.json.tmp").replace("probe.json")
time.sleep(90)
child.wait()
