#!/usr/bin/env python3
"""Проверка гостевого образа без демона и без игры.

    python3 test-boot.py <каталог образа> <рабочий каталог> [--gpu] [--wait 300]
        [--keys "meta_l-ret" ...] [--shots N] [--accel tcg|kvm]

Грузит образ в QEMU (слой qcow2 поверх базы, как это делает демон), пишет
вывод hvc0 в console.log, ждёт появления рабочего стола или приглашения
консоли, делает скриншоты через QMP screendump и, если задано, нажимает
клавиши (синтаксис QMP send-key: "meta_l-ret", "ctrl-alt-t").
"""
import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import threading
import time


class Qmp:
    def __init__(self, path, timeout=30):
        deadline = time.time() + timeout
        while True:
            try:
                self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.s.connect(path)
                break
            except OSError:
                if time.time() > deadline:
                    raise
                time.sleep(0.2)
        self.f = self.s.makefile("rw")
        json.loads(self.f.readline())  # greeting
        self.cmd("qmp_capabilities")

    def cmd(self, name, **args):
        msg = {"execute": name}
        if args:
            msg["arguments"] = args
        self.f.write(json.dumps(msg) + "\n")
        self.f.flush()
        while True:
            r = json.loads(self.f.readline())
            if "event" in r:
                continue
            if "error" in r:
                raise RuntimeError(f"{name}: {r['error']}")
            return r.get("return")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("work")
    ap.add_argument("--gpu", action="store_true")
    ap.add_argument("--wait", type=int, default=300)
    ap.add_argument("--keys", nargs="*", default=[])
    ap.add_argument("--after-keys-wait", type=int, default=60)
    ap.add_argument("--accel", default="kvm" if os.access("/dev/kvm", os.W_OK) else "tcg")
    ap.add_argument("--mem", type=int, default=2048)
    ap.add_argument("--smp", type=int, default=2)
    ap.add_argument("--append", default="")
    ap.add_argument("--console-cmd", nargs="*", default=[],
                    help="команды для текстовой консоли hvc0 после входа")
    a = ap.parse_args()

    man = json.load(open(os.path.join(a.image, "manifest.json")))
    os.makedirs(a.work, exist_ok=True)
    disk = os.path.join(a.work, "disk.qcow2")
    if os.path.exists(disk):
        os.remove(disk)
    subprocess.run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b",
                    os.path.abspath(os.path.join(a.image, man["rootfs"])), disk], check=True)
    qmp_path = os.path.join(a.work, "qmp.sock")
    con_sock = os.path.join(a.work, "con.sock")
    con_log = os.path.join(a.work, "console.log")
    for p in (qmp_path, con_log, con_sock):
        if os.path.exists(p):
            os.remove(p)

    accel = ["-accel", "kvm", "-cpu", "host"] if a.accel == "kvm" else \
            ["-accel", "tcg,thread=multi", "-cpu", "max"]
    cmd = ["qemu-system-x86_64", "-name", "fcw-test", "-machine", "q35", *accel,
           "-smp", str(a.smp), "-m", str(a.mem), "-nodefaults", "-no-user-config",
           "-display", "none",
           "-kernel", os.path.join(a.image, man["kernel"]),
           "-initrd", os.path.join(a.image, man["initrd"]),
           "-append", (man["cmdline"] + f" systemd.hostname=fcw-{man['id']} " + a.append).strip(),
           "-drive", f"if=none,id=root,file={disk},format=qcow2,discard=unmap",
           "-device", "virtio-blk-pci,drive=root",
           "-device", "virtio-serial-pci",
           "-chardev", f"socket,id=con0,path={con_sock},server=on,wait=off",
           "-device", "virtconsole,chardev=con0",
           "-object", "rng-random,id=rng0,filename=/dev/urandom",
           "-device", "virtio-rng-pci,rng=rng0",
           "-qmp", f"unix:{qmp_path},server=on,wait=off"]
    if a.gpu:
        cmd += ["-vga", "none", "-device", "virtio-gpu-pci,xres=1024,yres=768,edid=on",
                "-device", "virtio-keyboard-pci", "-device", "virtio-tablet-pci"]
    print("$", " ".join(cmd), flush=True)
    t0 = time.time()
    proc = subprocess.Popen(cmd, stdout=open(os.path.join(a.work, "qemu.log"), "w"),
                            stderr=subprocess.STDOUT)
    con = None
    try:
        q = Qmp(qmp_path)
        con = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        con.connect(con_sock)
        logf = open(con_log, "wb")

        def pump():
            while True:
                try:
                    b = con.recv(4096)
                except OSError:
                    return
                if not b:
                    return
                logf.write(b)
                logf.flush()
        threading.Thread(target=pump, daemon=True).start()
        shot_no = 0

        def shot(tag):
            nonlocal shot_no
            if not a.gpu:
                return
            shot_no += 1
            path = os.path.abspath(os.path.join(a.work, f"shot{shot_no:02d}-{tag}.png"))
            q.cmd("screendump", filename=path, format="png")
            print(f"[{time.time()-t0:6.1f}s] скриншот {path}", flush=True)

        prompt_at = None
        deadline = t0 + a.wait
        next_shot = t0 + 30
        while time.time() < deadline:
            if proc.poll() is not None:
                print("QEMU завершился, код", proc.returncode)
                break
            log = open(con_log, errors="replace").read() if os.path.exists(con_log) else ""
            if prompt_at is None and ("user@fcw-" in log):
                prompt_at = time.time()
                print(f"[{prompt_at-t0:6.1f}s] приглашение консоли hvc0 появилось", flush=True)
                if not a.gpu:
                    break
            if a.gpu and time.time() >= next_shot:
                shot(f"{int(time.time()-t0)}s")
                next_shot = time.time() + 30
            if a.gpu and prompt_at and time.time() - prompt_at > 45:
                break
            time.sleep(1)
        shot("ready")
        for c in a.console_cmd:
            con.sendall(c.encode() + b"\n")
            print(f"[{time.time()-t0:6.1f}s] hvc0$ {c}", flush=True)
            time.sleep(5)
        for k in a.keys:
            q.cmd("send-key", keys=[{"type": "qcode", "data": x} for x in k.split("-")])
            print(f"[{time.time()-t0:6.1f}s] клавиши {k}", flush=True)
            time.sleep(2)
        if a.keys:
            time.sleep(a.after_keys_wait)
            shot("after-keys")
        q.cmd("system_powerdown")
        try:
            proc.wait(timeout=90)
        except subprocess.TimeoutExpired:
            q.cmd("quit")
    finally:
        if proc.poll() is None:
            proc.kill()
    print("--- конец console.log ---")
    log = open(con_log, errors="replace").read() if os.path.exists(con_log) else ""
    print(log[-3000:])


if __name__ == "__main__":
    main()
