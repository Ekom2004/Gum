from __future__ import annotations

import json
import os
import shlex
import socket
import subprocess
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

from .models import JobRecord, TransformSpec


@dataclass
class LaunchState:
    bind_addr: str
    run_dir: Path
    coordinator: subprocess.Popen[bytes]
    agents: list[subprocess.Popen[bytes]] = field(default_factory=list)
    next_agent_index: int = 0


class CoordinatorLauncher:
    def __init__(self) -> None:
        self._jobs: dict[str, LaunchState] = {}
        self._lock = threading.Lock()
        self._repo_root = Path(__file__).resolve().parent.parent
        self._run_root = self._repo_root / ".mx8-media" / "launches"
        self._run_root.mkdir(parents=True, exist_ok=True)

    def launch(self, record: JobRecord, api_base_url: str) -> None:
        with self._lock:
            state = self._jobs.get(record.id)
            if state is not None and state.coordinator.poll() is None:
                return

            bind_addr = self._allocate_bind_addr()
            run_dir = self._run_root / record.id
            run_dir.mkdir(parents=True, exist_ok=True)

            coordinator_env = self._base_env(record, api_base_url)
            coordinator_env.update(
                {
                    "MX8_COORD_BIND_ADDR": bind_addr,
                    "MX8_WORLD_SIZE": coordinator_env.get(
                        "MX8_WORLD_SIZE", str(self._max_local_workers())
                    ),
                    "MX8_MIN_WORLD_SIZE": coordinator_env.get("MX8_MIN_WORLD_SIZE", "1"),
                    "MX8_COORD_HA_ENABLE": coordinator_env.get("MX8_COORD_HA_ENABLE", "false"),
                    "MX8_COORD_STATE_STORE_ENABLE": coordinator_env.get(
                        "MX8_COORD_STATE_STORE_ENABLE", "false"
                    ),
                    "MX8_LEASE_LOG_PATH": coordinator_env.get("MX8_LEASE_LOG_PATH", "none"),
                    "MX8_DATASET_LINK": record.source,
                }
            )
            coordinator_log_path = run_dir / "coordinator.log"
            coordinator = self._spawn_checked(
                self._coordinator_command(record),
                coordinator_env,
                coordinator_log_path,
                f"coordinator for job {record.id}",
            )
            self._jobs[record.id] = LaunchState(
                bind_addr=bind_addr,
                run_dir=run_dir,
                coordinator=coordinator,
            )

        self._wait_for_tcp(bind_addr)

        if self._local_agents_enabled():
            initial_count = self._initial_local_agents()
            self.scale_local_agents(record, api_base_url, initial_count)

    def scale_local_agents(self, record: JobRecord, api_base_url: str, desired_agents: int) -> None:
        if not self._local_agents_enabled():
            return
        desired_agents = max(0, min(desired_agents, self._max_local_workers()))
        with self._lock:
            state = self._jobs.get(record.id)
            if state is None or state.coordinator.poll() is not None:
                return
            state.agents = [agent for agent in state.agents if agent.poll() is None]
            current_agents = len(state.agents)

            while current_agents < desired_agents:
                index = state.next_agent_index
                state.next_agent_index += 1
                agent = self._spawn_agent(record, api_base_url, state, index)
                state.agents.append(agent)
                current_agents += 1

            while current_agents > desired_agents:
                agent = state.agents.pop()
                if agent.poll() is None:
                    agent.terminate()
                current_agents -= 1

    def terminate_job(self, job_id: str) -> None:
        with self._lock:
            state = self._jobs.pop(job_id, None)
        if state is None:
            return
        for agent in state.agents:
            if agent.poll() is None:
                agent.terminate()
        if state.coordinator.poll() is None:
            state.coordinator.terminate()

    def terminate_all(self) -> None:
        with self._lock:
            job_ids = list(self._jobs)
        for job_id in job_ids:
            self.terminate_job(job_id)

    def _coordinator_command(self, record: JobRecord) -> list[str]:
        raw = os.getenv("MX8_COORDINATOR_CMD", "").strip()
        if raw:
            return shlex.split(raw)
        command = ["cargo", "run", "-p", "mx8-coordinator"]
        if self._needs_s3(record):
            command.extend(["--features", "s3"])
        command.append("--")
        return command

    def _agent_command(self, record: JobRecord) -> list[str]:
        raw = os.getenv("MX8_AGENT_CMD", "").strip()
        if raw:
            return shlex.split(raw)
        command = ["cargo", "run", "-p", "mx8d-agent"]
        if self._needs_s3(record):
            command.extend(["--features", "s3"])
        command.append("--")
        return command

    def _allocate_bind_addr(self) -> str:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.bind(("127.0.0.1", 0))
            host, port = sock.getsockname()
        return f"{host}:{port}"

    def _base_env(self, record: JobRecord, api_base_url: str) -> dict[str, str]:
        env = os.environ.copy()
        env.update(
            {
                "MX8_JOB_ID": record.id,
                "MX8_SOURCE_URI": record.source,
                "MX8_SINK_URI": record.sink,
                "MX8_AWS_REGION": env.get("MX8_AWS_REGION", "us-east-1"),
                "MX8_TRANSFORMS_JSON": json.dumps(
                    [self._rust_transform_json(transform) for transform in record.transforms]
                ),
                "MX8_API_BASE_URL": api_base_url.rstrip("/"),
            }
        )
        return env

    def _spawn_checked(
        self,
        command: list[str],
        env: dict[str, str],
        log_path: Path,
        label: str,
    ) -> subprocess.Popen[bytes]:
        log_handle = log_path.open("ab")
        process = subprocess.Popen(
            command,
            cwd=self._repo_root,
            env=env,
            stdout=log_handle,
            stderr=subprocess.STDOUT,
        )
        time.sleep(0.5)
        return_code = process.poll()
        if return_code is not None:
            log_handle.close()
            raise RuntimeError(
                f"{label} exited immediately with status {return_code}; see {log_path}"
            )
        return process

    def _wait_for_tcp(self, bind_addr: str, timeout_secs: float | None = None) -> None:
        if timeout_secs is None:
            timeout_secs = float(os.getenv("MX8_LAUNCH_WAIT_SECS", "90"))
        host, port_text = bind_addr.rsplit(":", 1)
        deadline = time.time() + timeout_secs
        last_error: Exception | None = None
        while time.time() < deadline:
            try:
                with socket.create_connection((host, int(port_text)), timeout=0.5):
                    return
            except OSError as err:
                last_error = err
                time.sleep(0.1)
        raise RuntimeError(f"coordinator did not start listening on {bind_addr}: {last_error}")

    def _local_agents_enabled(self) -> bool:
        raw = os.getenv("MX8_LOCAL_AGENT_ENABLE", "true").strip().lower()
        return raw not in {"0", "false", "no", "off"}

    def _initial_local_agents(self) -> int:
        raw = os.getenv("MX8_LOCAL_AGENT_INITIAL_COUNT", "").strip()
        if raw:
            return max(0, int(raw))
        legacy = os.getenv("MX8_LOCAL_AGENT_COUNT", "").strip()
        if legacy:
            return max(0, int(legacy))
        return 1

    def _max_local_workers(self) -> int:
        raw = os.getenv("MX8_SCALE_MAX_WORKERS", "").strip()
        if raw:
            return max(1, int(raw))
        return 8

    def _needs_s3(self, record: JobRecord) -> bool:
        return record.source.startswith("s3://") or record.sink.startswith("s3://")

    def _spawn_agent(
        self,
        record: JobRecord,
        api_base_url: str,
        state: LaunchState,
        index: int,
    ) -> subprocess.Popen[bytes]:
        agent_env = self._base_env(record, api_base_url)
        agent_env.update(
            {
                "MX8_COORD_URL": f"http://{state.bind_addr}",
                "MX8_NODE_ID": f"local-node-{index}",
                "MX8_DEV_LEASE_WANT": agent_env.get("MX8_DEV_LEASE_WANT", "1"),
            }
        )
        agent_log_path = state.run_dir / f"agent-{index}.log"
        return self._spawn_checked(
            self._agent_command(record),
            agent_env,
            agent_log_path,
            f"agent {index} for job {record.id}",
        )

    def _rust_transform_json(self, transform: TransformSpec) -> dict[str, object]:
        params = dict(transform.params)
        if transform.type == "image.resize":
            return {
                "ImageResize": {
                    "width": params["width"],
                    "height": params["height"],
                    "maintain_aspect": params.get("maintain_aspect", True),
                }
            }
        if transform.type == "image.crop":
            return {
                "ImageCrop": {
                    "width": params["width"],
                    "height": params["height"],
                }
            }
        if transform.type == "image.convert":
            return {
                "ImageConvert": {
                    "format": params["format"],
                    "quality": params.get("quality", 85),
                }
            }
        if transform.type == "video.transcode":
            return {
                "VideoTranscode": {
                    "codec": params["codec"],
                    "crf": params.get("crf", 23),
                }
            }
        if transform.type == "video.resize":
            return {
                "VideoResize": {
                    "width": params["width"],
                    "height": params["height"],
                    "maintain_aspect": params.get("maintain_aspect", True),
                }
            }
        if transform.type == "video.extract_audio":
            return {
                "VideoExtractAudio": {
                    "format": params["format"],
                    "bitrate": params.get("bitrate", "128k"),
                }
            }
        if transform.type == "video.extract_frames":
            return {
                "VideoExtractFrames": {
                    "fps": params["fps"],
                    "format": params["format"],
                }
            }
        if transform.type == "audio.resample":
            return {
                "AudioResample": {
                    "rate": params["rate"],
                    "channels": params.get("channels", 1),
                }
            }
        if transform.type == "audio.normalize":
            return {
                "AudioNormalize": {
                    "loudness_lufs": params.get("loudness", -14.0),
                }
            }
        raise ValueError(f"unsupported transform type for coordinator launch: {transform.type}")
