"""Sends WM_DELETE_WINDOW to every top-level window named VoxelCraft*, as a window manager's close button does."""

import ctypes
import sys

x = ctypes.cdll.LoadLibrary("libX11.so.6")
x.XOpenDisplay.restype = ctypes.c_void_p
x.XDefaultRootWindow.restype = ctypes.c_ulong
x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x.XInternAtom.restype = ctypes.c_ulong
x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]


class ClientMessage(ctypes.Structure):
    _fields_ = [
        ("type", ctypes.c_int),
        ("serial", ctypes.c_ulong),
        ("send_event", ctypes.c_int),
        ("display", ctypes.c_void_p),
        ("window", ctypes.c_ulong),
        ("message_type", ctypes.c_ulong),
        ("format", ctypes.c_int),
        ("data", ctypes.c_long * 5),
    ]


class Event(ctypes.Union):
    _fields_ = [("xclient", ClientMessage), ("pad", ctypes.c_long * 24)]


d = x.XOpenDisplay(None)
if not d:
    sys.exit("no display")
protocols = x.XInternAtom(d, b"WM_PROTOCOLS", 0)
delete = x.XInternAtom(d, b"WM_DELETE_WINDOW", 0)
x.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(Event)]
x.XFlush.argtypes = [ctypes.c_void_p]
sent = 0
for wid in sys.argv[1:]:
    e = Event()
    e.xclient.type = 33  # ClientMessage
    e.xclient.window = int(wid)
    e.xclient.message_type = protocols
    e.xclient.format = 32
    e.xclient.data[0] = delete
    e.xclient.data[1] = 0  # CurrentTime
    x.XSendEvent(d, int(wid), 0, 0, ctypes.byref(e))
    sent += 1
x.XFlush(d)
print(f"sent WM_DELETE_WINDOW to {sent} window(s)")
