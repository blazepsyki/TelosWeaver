"""Schema-less protobuf wire reader/writer (varint, 64-bit, length-delimited, 32-bit)."""
def varint(b, i):
    r = s = 0
    while True:
        x = b[i]; i += 1; r |= (x & 0x7F) << s; s += 7
        if x < 0x80: return r, i
def fields(b):
    """Yield (field_no, wire_type, value_bytes_or_int, raw_bytes) for one message."""
    i = 0
    while i < len(b):
        start = i
        key, i = varint(b, i); fn, wt = key >> 3, key & 7
        if wt == 0: v, i = varint(b, i)
        elif wt == 1: v = b[i:i+8]; i += 8
        elif wt == 2:
            n, i = varint(b, i); v = b[i:i+n]; i += n
        elif wt == 5: v = b[i:i+4]; i += 4
        else: raise ValueError(f"wire type {wt} at {start}")
        yield fn, wt, v, b[start:i]
