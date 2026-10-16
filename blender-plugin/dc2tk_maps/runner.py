import subprocess
import sys
import threading

import bpy

TEXT_NAME = "dc2tk output"


current = None


def new_job(title, command, cwd):
    return {
        "title": title,
        "command": command,
        "cwd": cwd,
        "lines": [],
        "returncode": None,
        "process": None,
        "lock": threading.Lock(),
        "thread": None,
    }


def start_job(job):
    flags = 0
    if sys.platform.startswith("win"):
        flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    job["process"] = subprocess.Popen(
        job["command"], cwd=job["cwd"], stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL, creationflags=flags,
    )
    job["thread"] = threading.Thread(target=read_output, args=(job,), daemon=True)
    job["thread"].start()


def read_output(job):
    process = job["process"]
    for raw in process.stdout:
        line = raw.decode("utf-8", errors="replace").rstrip("\r\n")
        with job["lock"]:
            job["lines"].append(line)
    process.stdout.close()
    process.wait()
    with job["lock"]:
        job["returncode"] = process.returncode


def output_lines(job):
    with job["lock"]:
        return list(job["lines"])


def job_done(job):
    with job["lock"]:
        return job["returncode"] is not None


def describe(command):
    return " ".join('"%s"' % part if " " in part else part for part in command)


def write_text(job):
    text = bpy.data.texts.get(TEXT_NAME) or bpy.data.texts.new(TEXT_NAME)
    text.clear()
    text.write("$ %s\n" % describe(job["command"]))
    text.write("\n".join(output_lines(job)))
    if job["returncode"] is not None:
        text.write("\n[exit code %d]\n" % job["returncode"])
    return text


def run_blocking(title, command, cwd):
    global current
    job = new_job(title, command, cwd)
    current = job
    start_job(job)
    job["thread"].join()
    write_text(job)
    return job


def tail(count=12):
    if current is None:
        return []
    lines = output_lines(current)
    return lines[-count:]


def execute(operator, context):
    planned = operator.command(context)
    if planned is None:
        return {"CANCELLED"}
    try:
        job = run_blocking(*planned)
    except OSError as error:
        operator.report({"ERROR"}, "Could not run dc2tk: %s" % error)
        return {"CANCELLED"}
    return operator.finish(context, job)


def invoke(operator, context):
    if bpy.app.background:
        return execute(operator, context)
    planned = operator.command(context)
    if planned is None:
        return {"CANCELLED"}
    global current
    operator._job = new_job(*planned)
    current = operator._job
    try:
        start_job(operator._job)
    except OSError as error:
        operator.report({"ERROR"}, "Could not run dc2tk: %s" % error)
        return {"CANCELLED"}
    operator._timer = context.window_manager.event_timer_add(0.25, window=context.window)
    context.window_manager.modal_handler_add(operator)
    operator.report({"INFO"}, "%s: running dc2tk" % planned[0])
    return {"RUNNING_MODAL"}


def modal(operator, context, event):
    if event.type != "TIMER":
        return {"PASS_THROUGH"}
    for area in context.screen.areas if context.screen else []:
        if area.type in {"VIEW_3D", "TEXT_EDITOR"}:
            area.tag_redraw()
    if not job_done(operator._job):
        return {"PASS_THROUGH"}
    context.window_manager.event_timer_remove(operator._timer)
    write_text(operator._job)
    return operator.finish(context, operator._job)
