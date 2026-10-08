"""Reserve benchmark cores on the disposable nightly AWS hosts (run as root)."""

import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import time


def cpus(value):
    result = set()
    for part in value.strip().split(','):
        first, _, last = part.partition('-')
        result.update(range(int(first), int(last or first) + 1))
    return result


def select_cores(allowed, siblings, count):
    groups = sorted({tuple(sorted(siblings[cpu])) for cpu in allowed})
    # Leave the first physical core for housekeeping; reserve SMT siblings too.
    chosen = groups[1:count + 1]
    if len(chosen) != count:
        raise RuntimeError('Not enough physical cores for benchmark and housekeeping')
    selected = [min(set(group) & allowed) for group in chosen]
    reserved = set().union(*map(set, chosen))
    return selected, reserved


def wrap_client(path, selected):
    path = Path(path).resolve(strict=True)
    original = path.with_name(path.name + '.redisjson-original')
    if original.exists():
        raise RuntimeError(f'Client already wrapped: {path}')
    taskset = shutil.which('taskset')
    if not taskset:
        raise RuntimeError('taskset is required')
    wrapper = '#!/bin/sh\nexec ' + shlex.join([
        taskset, '-c', ','.join(map(str, selected)), str(original)
    ]) + ' "$@"\n'
    path.rename(original)
    path.write_text(wrapper)
    path.chmod(0o755)
    return str(path)


def wait_for_irq_affinity(targets):
    # IRQ migration can be deferred until a subsequent interrupt. Writing the
    # requested mask does not guarantee effective_affinity changes immediately.
    deadline = time.monotonic() + 10
    while True:
        effective = {irq: (Path('/proc/irq') / irq / 'effective_affinity_list').read_text().strip()
                     for irq in targets}
        pending = {irq: {'requested': target, 'effective': effective[irq]}
                   for irq, target in targets.items() if cpus(effective[irq]) != {target}}
        if not pending:
            return effective
        if time.monotonic() >= deadline:
            raise RuntimeError(f'IRQ affinity did not settle within 10s: {pending}')
        time.sleep(0.1)


def configure(role, clients):
    allowed = set(os.sched_getaffinity(0))
    siblings = {cpu: cpus(Path(f'/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list').read_text())
                for cpu in allowed}
    selected, reserved = select_cores(allowed, siblings, 1 if role == 'server' else 2)
    housekeeping = sorted(allowed - reserved)
    interfaces = [path for path in Path('/sys/class/net').iterdir()
                  if (path / 'device/msi_irqs').is_dir()]
    if not interfaces:
        raise RuntimeError('No NIC MSI interrupts found; cannot verify isolation')
    active = subprocess.run(['systemctl', 'is-active', '--quiet', 'irqbalance']).returncode == 0
    if active:
        subprocess.run(['systemctl', 'stop', 'irqbalance'], check=True)
    irqs = {}
    for interface in interfaces:
        for index, irq in enumerate(sorted((interface / 'device/msi_irqs').iterdir())):
            directory = Path('/proc/irq') / irq.name
            target = housekeeping[index % len(housekeeping)]
            (directory / 'smp_affinity_list').write_text(str(target))
            requested = (directory / 'smp_affinity_list').read_text().strip()
            if cpus(requested) != {target}:
                raise RuntimeError(f'IRQ {irq.name} rejected CPU {target}: requested={requested}')
            irqs[irq.name] = target
        # Disable software packet steering, which can otherwise bypass IRQ affinity.
        for rps in (interface / 'queues').glob('rx-*/rps_cpus'):
            rps.write_text('0')
            if int(rps.read_text().strip().replace(',', ''), 16):
                raise RuntimeError(f'RPS still enabled: {rps}')
    irqs = wait_for_irq_affinity(irqs)
    data = dict(role=role, cpulist=','.join(map(str, selected)),
                reserved_cpus=sorted(reserved), nic_irqs=irqs, irqbalance_stopped=active,
                clients=[wrap_client(path, selected) for path in clients])
    Path('/tmp/redisjson-affinity.json').write_text(json.dumps(data))
    return data


if __name__ == '__main__':
    print(json.dumps(configure(sys.argv[1], sys.argv[2:])))
