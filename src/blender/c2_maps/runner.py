"""Running c2: in the background with its output streamed into a text block (from the UI), or to
the end (from scripts and in background mode)."""

import subprocess
import sys
import threading

import bpy

TEXT_NAME = "c2 output"
MAX_LINES = 400


class Job:
    def __init__(self, title, command, cwd):
        self.title = title
        self.command = command
        self.cwd = cwd
        self.lines = []
        self.returncode = None
        self.process = None
        self._lock = threading.Lock()
        self._thread = None

    def start(self):
        flags = 0
        if sys.platform.startswith("win"):
            flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        self.process = subprocess.Popen(
            self.command,
            cwd=self.cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            creationflags=flags,
        )
        self._thread = threading.Thread(target=self._read, daemon=True)
        self._thread.start()

    def _read(self):
        for raw in self.process.stdout:
            line = raw.decode("utf-8", errors="replace").rstrip("\r\n")
            with self._lock:
                self.lines.append(line)
        self.process.wait()
        with self._lock:
            self.returncode = self.process.returncode

    def take(self):
        with self._lock:
            return list(self.lines)

    @property
    def done(self):
        with self._lock:
            return self.returncode is not None

    def wait(self):
        if self._thread is not None:
            self._thread.join()


# The last job, for the panel.
current = None


def describe(command):
    return " ".join('"%s"' % part if " " in part else part for part in command)


def write_text(job):
    text = bpy.data.texts.get(TEXT_NAME) or bpy.data.texts.new(TEXT_NAME)
    text.clear()
    text.write("$ %s\n" % describe(job.command))
    text.write("\n".join(job.take()))
    if job.returncode is not None:
        text.write("\n[exit code %d]\n" % job.returncode)
    return text


def run_blocking(title, command, cwd):
    """Run to the end; the job (with its lines and exit code)."""
    global current
    job = Job(title, command, cwd)
    current = job
    job.start()
    job.wait()
    write_text(job)
    return job


def tail(count=12):
    if current is None:
        return []
    lines = current.take()
    return lines[-count:]


class C2_OT_run_job(bpy.types.Operator):
    """Base of the operators that run c2 in the background and then finish up"""

    bl_idname = "c2.run_job_base"
    bl_label = "Run c2"
    bl_options = {"INTERNAL"}

    _timer = None
    _job = None

    def command(self, context):
        """(title, command list, cwd), or None after reporting why not."""
        raise NotImplementedError

    def finish(self, context, job):
        """Called when the job ended; returns the operator result set."""
        return {"FINISHED"} if job.returncode == 0 else {"CANCELLED"}

    def execute(self, context):
        planned = self.command(context)
        if planned is None:
            return {"CANCELLED"}
        job = run_blocking(*planned)
        return self.finish(context, job)

    def invoke(self, context, event):
        if bpy.app.background:
            return self.execute(context)
        planned = self.command(context)
        if planned is None:
            return {"CANCELLED"}
        global current
        self._job = Job(*planned)
        current = self._job
        try:
            self._job.start()
        except OSError as error:
            self.report({"ERROR"}, "Could not run c2: %s" % error)
            return {"CANCELLED"}
        self._timer = context.window_manager.event_timer_add(0.25, window=context.window)
        context.window_manager.modal_handler_add(self)
        self.report({"INFO"}, "%s: running c2" % planned[0])
        return {"RUNNING_MODAL"}

    def modal(self, context, event):
        if event.type != "TIMER":
            return {"PASS_THROUGH"}
        for area in context.screen.areas if context.screen else []:
            if area.type in {"VIEW_3D", "TEXT_EDITOR"}:
                area.tag_redraw()
        if not self._job.done:
            return {"PASS_THROUGH"}
        context.window_manager.event_timer_remove(self._timer)
        write_text(self._job)
        return self.finish(context, self._job)
