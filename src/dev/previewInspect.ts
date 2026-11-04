/**
 * Preview-only stand-ins for the inspection commands (ports, open files,
 * modules, memory map, threads, file finder), so those views can be reviewed
 * in a plain browser. Loaded only by the dev preview.
 */
import type {
  FileUserList,
  HardwareInfo,
  MemoryMap,
  MemoryRegion,
  ModuleInfo,
  OpenFileList,
  SocketEntry,
  SocketList,
  StartupItem,
  ThreadList,
} from "@/types";

const socket = (
  protocol: "tcp" | "tcp6" | "udp",
  localPort: number,
  state: "listen" | "established" | null,
  pid: number,
  processName: string,
  localAddress = "127.0.0.1",
  remote: [string, number] | null = null,
): SocketEntry => ({
  connection: {
    protocol,
    localAddress,
    localPort,
    remoteAddress: remote?.[0] ?? null,
    remotePort: remote?.[1] ?? null,
    state,
  },
  pid,
  processName,
});

const SOCKETS: SocketList = {
  sockets: [
    socket("tcp", 3000, "listen", 28119, "node"),
    socket("tcp6", 3000, "listen", 28119, "node", "::1"),
    socket("tcp", 5173, "listen", 28100, "node", "127.0.0.1"),
    socket("tcp", 8080, "listen", 900, "java", "0.0.0.0"),
    socket("tcp", 22, "listen", 812, "sshd", "0.0.0.0"),
    socket("tcp", 631, "listen", 640, "cupsd", "127.0.0.1"),
    socket("udp", 5353, null, 700, "avahi-daemon", "0.0.0.0"),
    socket("tcp", 41532, "established", 2210, "firefox", "192.168.0.23", ["142.250.80.46", 443]),
    socket("tcp", 41544, "established", 2210, "firefox", "192.168.0.23", ["151.101.1.69", 443]),
  ],
  inaccessibleProcesses: 3,
  truncated: false,
};

const FILES: OpenFileList = {
  files: [
    { descriptor: "0", kind: "device", target: "/dev/null", access: "read" },
    { descriptor: "1", kind: "pipe", target: "pipe:[48211]", access: "write" },
    { descriptor: "3", kind: "file", target: "/home/demo/Videos/video.mp4", access: "read" },
    { descriptor: "4", kind: "file", target: "/home/demo/Videos/out.mkv", access: "readWrite" },
    { descriptor: "5", kind: "directory", target: "/home/demo/Videos", access: "read" },
    { descriptor: "7", kind: "socket", target: "socket:[91233]", access: null },
    { descriptor: "9", kind: "anonymous", target: "anon_inode:[eventpoll]", access: "read" },
  ],
  truncated: false,
};

const region = (
  start: number,
  size: number,
  permissions: string,
  kind: MemoryRegion["kind"],
  path: string | null,
): MemoryRegion => ({
  start: start.toString(16),
  end: (start + size).toString(16),
  size,
  permissions,
  offset: 0,
  path,
  kind,
  shared: permissions.endsWith("s"),
});

const MB = 1024 * 1024;
const REGIONS: MemoryRegion[] = [
  region(0x55d0c8a00000, 2 * MB, "r-xp", "image", "/usr/bin/demo"),
  region(0x55d0c9000000, 1400 * MB, "rw-p", "heap", "[heap]"),
  region(0x7f3a1c000000, 3 * MB, "r-xp", "sharedLibrary", "/usr/lib/x86_64-linux-gnu/libc.so.6"),
  region(0x7f3a1c400000, 720 * MB, "rw-s", "mappedFile", "/home/demo/.cache/index.db"),
  region(0x7f3a30000000, 100 * MB, "rw-p", "anonymous", null),
  region(0x7ffd1a3c5000, 132 * 1024, "rw-p", "stack", "[stack]"),
  region(0x7ffd1a3e6000, 16 * 1024, "r--p", "special", "[vvar]"),
];

function memoryMap(): MemoryMap {
  const byKind = new Map<MemoryRegion["kind"], { bytes: number; regions: number }>();
  for (const r of REGIONS) {
    const e = byKind.get(r.kind) ?? { bytes: 0, regions: 0 };
    byKind.set(r.kind, { bytes: e.bytes + r.size, regions: e.regions + 1 });
  }
  return {
    regions: REGIONS,
    summary: [...byKind].map(([kind, v]) => ({ kind, ...v })).sort((a, b) => b.bytes - a.bytes),
    totalBytes: REGIONS.reduce((a, r) => a + r.size, 0),
    executableBytes: 5 * MB,
    truncated: false,
  };
}

const MODULES: ModuleInfo[] = [
  { name: "demo", path: "/usr/bin/demo", base: "55d0c8a00000", size: 2 * MB, executable: true },
  {
    name: "libc.so.6",
    path: "/usr/lib/x86_64-linux-gnu/libc.so.6",
    base: "7f3a1c000000",
    size: 2 * MB,
    executable: true,
  },
  {
    name: "libz.so.1",
    path: "/usr/lib/x86_64-linux-gnu/libz.so.1",
    base: "7f3a1c600000",
    size: 120 * 1024,
    executable: true,
  },
];

function threads(): ThreadList {
  const now = Date.now();
  return {
    sampledAtMs: now,
    threads: [
      ["demo", "running", 6.2, 41_200, 3, null],
      ["worker-1", "running", 2.4, 18_900, 7, null],
      ["worker-2", "sleeping", 0.1, 5_400, 1, "futex_wait_queue"],
      ["io-thread", "diskSleep", 0, 1_200, 5, "io_schedule"],
      ["gc", "sleeping", 0, 900, 0, "do_epoll_wait"],
    ].map(([name, state, cpu, ms, core, wait], i) => ({
      tid: 28119 + i,
      name: String(name),
      state: state as "running",
      cpuPercent: Number(cpu) + Math.random() * 0.4,
      cpuTimeMs: Number(ms),
      priority: 0,
      startTime: Math.floor(now / 1000) - 3_600,
      processor: Number(core),
      waitChannel: wait == null ? null : String(wait),
    })),
  };
}

function fileUsers(path: string): FileUserList {
  const isDirectory = !/\.[a-z0-9]{1,5}$/i.test(path);
  return {
    resolvedPath: path,
    isDirectory,
    uses: [
      { pid: 18341, processName: "resolve", usage: "open", path },
      { pid: 19122, processName: "ffmpeg", usage: "open", path },
      { pid: 19122, processName: "ffmpeg", usage: "workingDirectory", path: "/home/demo/Videos" },
    ],
    inaccessibleProcesses: 2,
    truncated: false,
    note: null,
  };
}

const HARDWARE: HardwareInfo = {
  machine: {
    vendor: "LENOVO",
    model: "ThinkPad X1 Carbon Gen 11",
    board: "LENOVO 21HM",
    biosVendor: "LENOVO",
    biosVersion: "N3XET54W (1.30)",
    biosDate: "03/14/2024",
    chassis: "Laptop",
  },
  cpu: {
    maxMhz: 4_800,
    minMhz: 400,
    governor: "powersave",
    caches: [
      { level: 1, kind: "Data", sizeBytes: 48 * 1024 },
      { level: 1, kind: "Instruction", sizeBytes: 32 * 1024 },
      { level: 2, kind: "Unified", sizeBytes: 1_280 * 1024 },
      { level: 3, kind: "Unified", sizeBytes: 12 * 1024 * 1024 },
    ],
    virtualization: "Intel VT-x",
    virtualized: false,
  },
  batteries: [
    {
      name: "BAT0",
      percent: 82,
      status: "Discharging",
      technology: "Li-poly",
      cycleCount: 143,
      healthPercent: 91,
    },
  ],
  powerSource: "battery",
};

const startup = (
  name: string,
  command: string,
  scope: StartupItem["scope"],
  enabled: boolean,
  comment: string | null = null,
  onlyIn: string | null = null,
): StartupItem => ({
  id: `${name.toLowerCase().replace(/\W+/g, "-")}.desktop`,
  name,
  comment,
  command,
  scope,
  enabled,
  onlyIn,
});

/** Mutable so the Startup page's switches work in the preview. */
const STARTUP_ITEMS: StartupItem[] = [
  startup("Dropbox", "dropbox start", "user", true, "Sync your files"),
  startup("Syncthing", "syncthing serve --no-browser", "user", true),
  startup(
    "Power Management",
    "/usr/lib/x86_64-linux-gnu/libexec/org_kde_powerdevil",
    "system",
    true,
  ),
  startup("Desktop Shell", "plasmashell --no-respawn", "system", true, "Plasma desktop"),
  startup(
    "Secret Storage Service",
    "/usr/bin/gnome-keyring-daemon --start",
    "system",
    true,
    null,
    "GNOME, Unity, MATE",
  ),
  startup("File Indexer", "/usr/libexec/baloo_file", "system", false, "Indexes files for search"),
  startup("Spice vdagent", "/usr/bin/spice-vdagent", "system", false, "Agent for Spice guests"),
];

const NOT_HANDLED = Symbol("not handled");

/** Returns the stub reply for an inspection command, or `NOT_HANDLED`. */
export function inspectPreview(cmd: string, args: unknown): unknown {
  switch (cmd) {
    case "list_startup_items":
      return STARTUP_ITEMS.map((i) => ({ ...i }));
    case "set_startup_enabled": {
      const { id, enabled } = args as { id: string; enabled: boolean };
      const item = STARTUP_ITEMS.find((i) => i.id === id);
      if (item) item.enabled = enabled;
      return null;
    }
    case "list_sockets":
      return SOCKETS;
    case "get_open_files":
      return FILES;
    case "get_memory_map":
      return memoryMap();
    case "get_modules":
      return MODULES;
    case "get_threads":
      return threads();
    case "find_file_users":
      return fileUsers((args as { path: string }).path);
    case "get_hardware_info":
      return HARDWARE;
    case "pick_path":
      return "/home/demo/Videos/video.mp4";
    case "get_activity":
      return [];
    default:
      return NOT_HANDLED;
  }
}

export { NOT_HANDLED };
