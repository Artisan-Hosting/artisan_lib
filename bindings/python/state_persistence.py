from dataclasses import dataclass, field, asdict
from typing import Optional, List, Tuple
import json


@dataclass
class Aggregator:
    socket_path: str
    socket_permission: Optional[int] = None


@dataclass
class GitConfig:
    default_server: str
    credentials_file: str


@dataclass
class DatabaseConfig:
    url: str
    pool_size: int


@dataclass
class AppConfig:
    app_name: str
    max_ram_usage: int
    max_cpu_usage: int
    environment: str
    debug_mode: bool
    log_level: str
    git: Optional[GitConfig] = None
    database: Optional[DatabaseConfig] = None
    aggregator: Optional[Aggregator] = None


@dataclass
class ErrorItem:
    err_type: str
    err_mesg: str


@dataclass
class AppState:
    name: str
    version: str
    data: str
    status: str
    pid: int
    last_updated: int
    stared_at: int
    event_counter: int
    error_log: List[ErrorItem] = field(default_factory=list)
    config: AppConfig = field(default_factory=AppConfig)
    system_application: bool = False
    stdout: List[Tuple[int, str]] = field(default_factory=list)
    stderr: List[Tuple[int, str]] = field(default_factory=list)


class StatePersistence:
    """Simple JSON-based state persistence."""

    @staticmethod
    def save_state(state: AppState, path: str) -> None:
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(asdict(state), fh, indent=2)

    @staticmethod
    def load_state(path: str) -> AppState:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
        data["config"] = AppConfig(**data["config"])
        data["error_log"] = [ErrorItem(**e) for e in data.get("error_log", [])]
        return AppState(**data)
