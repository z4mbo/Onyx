import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
const terminalHandles = /* @__PURE__ */ new Map();
const terminalHandlesBySession = /* @__PURE__ */ new Map();
const terminalReplay = /* @__PURE__ */ new Map();
const closedTerminals = /* @__PURE__ */ new Set();
let nextTerminalHandle = 1;
let terminalRuntimeLoad = null;
const MAX_TERMINAL_REPLAY = 1048576;
const convexUrl = import.meta.env.VITE_CONVEX_URL?.trim() ?? "";
let convex = null;
let convexApi = null;
let convexLoad = null;
function nativeRuntimeAvailable() {
  return typeof window.__TAURI_INTERNALS__?.invoke === "function"
    || typeof window.__TAURI__?.core?.invoke === "function";
}
function terminalTheme(element) {
  const styles = getComputedStyle(element);
  return {
    background: "transparent",
    foreground: styles.color || "#e8e8e8",
    cursor: styles.color || "#e8e8e8",
    cursorAccent: styles.backgroundColor || "#111",
    selectionBackground: "rgba(120, 150, 255, 0.28)",
    black: "#202020",
    red: "#ff6b7a",
    green: "#77d991",
    yellow: "#e8c56b",
    blue: "#7db4ff",
    magenta: "#c79cff",
    cyan: "#69d9df",
    white: "#d7d7d7",
    brightBlack: "#777",
    brightRed: "#ff9aa5",
    brightGreen: "#a7e9b7",
    brightYellow: "#f6dc9b",
    brightBlue: "#a8ccff",
    brightMagenta: "#dfc0ff",
    brightCyan: "#9cebef",
    brightWhite: "#fff"
  };
}
async function loadTerminalRuntime() {
  terminalRuntimeLoad ??= Promise.all([
    import("@xterm/xterm"),
    import("@xterm/addon-fit")
  ]).then(([xterm, addonFit]) => ({
    Terminal: xterm.Terminal,
    FitAddon: addonFit.FitAddon
  })).catch((error) => {
    terminalRuntimeLoad = null;
    throw error;
  });
  return await terminalRuntimeLoad;
}
async function mountTerminal(element, sessionId, onData, onResize, autofocus) {
  if (!element.isConnected) {
    throw new Error("Terminal view was closed before it finished loading.");
  }
  const { Terminal, FitAddon } = await loadTerminalRuntime();
  if (!element.isConnected) {
    throw new Error("Terminal view was closed before it finished loading.");
  }
  const terminal = new Terminal({
    allowTransparency: true,
    convertEol: false,
    cursorBlink: true,
    cursorStyle: "block",
    fontFamily: '"JetBrains Mono", "SFMono-Regular", Menlo, Consolas, monospace',
    fontSize: 12,
    lineHeight: 1.25,
    minimumContrastRatio: 4.5,
    scrollback: 5e3,
    theme: terminalTheme(element)
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.open(element);
  const replay = terminalReplay.get(sessionId);
  if (replay) terminal.write(replay);
  if (closedTerminals.has(sessionId)) terminal.options.disableStdin = true;
  const handleId = nextTerminalHandle++;
  const resize = () => {
    const handle = terminalHandles.get(handleId);
    if (!handle) return;
    cancelAnimationFrame(handle.resizeFrame);
    handle.resizeFrame = requestAnimationFrame(() => {
      if (!element.isConnected) return;
      try {
        fit.fit();
        if (terminal.cols > 0 && terminal.rows > 0) {
          onResize(terminal.cols, terminal.rows);
        }
      } catch {
      }
    });
  };
  const observer = new ResizeObserver(resize);
  observer.observe(element);
  const dataSubscription = terminal.onData((data) => {
    if (!closedTerminals.has(sessionId)) onData(data);
  });
  terminalHandles.set(handleId, {
    terminal,
    fit,
    observer,
    dataSubscription,
    resizeFrame: 0
  });
  const sessionHandles = terminalHandlesBySession.get(sessionId) ?? /* @__PURE__ */ new Set();
  sessionHandles.add(handleId);
  terminalHandlesBySession.set(sessionId, sessionHandles);
  resize();
  if (autofocus) {
    queueMicrotask(() => {
      if (element.isConnected && terminalHandles.has(handleId)) terminal.focus();
    });
  }
  return handleId;
}
function unmountTerminal(handleId, sessionId) {
  const handle = terminalHandles.get(handleId);
  if (!handle) return;
  cancelAnimationFrame(handle.resizeFrame);
  handle.observer.disconnect();
  handle.dataSubscription.dispose();
  handle.terminal.dispose();
  terminalHandles.delete(handleId);
  const sessionHandles = terminalHandlesBySession.get(sessionId);
  sessionHandles?.delete(handleId);
  if (sessionHandles?.size === 0) terminalHandlesBySession.delete(sessionId);
}
function appendTerminalReplay(sessionId, data) {
  const next = `${terminalReplay.get(sessionId) ?? ""}${data}`;
  terminalReplay.set(
    sessionId,
    next.length > MAX_TERMINAL_REPLAY ? next.slice(next.length - MAX_TERMINAL_REPLAY) : next
  );
}
function writeTerminal(sessionId, data) {
  appendTerminalReplay(sessionId, data);
  terminalHandlesBySession.get(sessionId)?.forEach((handleId) => {
    terminalHandles.get(handleId)?.terminal.write(data);
  });
}
function exitTerminal(sessionId, exitCode, error) {
  closedTerminals.add(sessionId);
  const suffix = error ? `\r
\x1B[31m[terminal] ${error}\x1B[0m\r
` : `\r
\x1B[2m[process exited${exitCode == null ? "" : ` with code ${exitCode}`}]\x1B[0m\r
`;
  writeTerminal(sessionId, suffix);
  terminalHandlesBySession.get(sessionId)?.forEach((handleId) => {
    const terminal = terminalHandles.get(handleId)?.terminal;
    if (terminal) terminal.options.disableStdin = true;
  });
}
function clearTerminal(sessionId) {
  terminalReplay.set(sessionId, "");
  terminalHandlesBySession.get(sessionId)?.forEach((handleId) => {
    terminalHandles.get(handleId)?.terminal.clear();
  });
}
function forgetTerminal(sessionId) {
  terminalReplay.delete(sessionId);
  closedTerminals.delete(sessionId);
}
const TARGET_SAMPLE_RATE = 16e3;
const MAX_CAPTURE_SECONDS = 5 * 60;
class SpeechCapture {
  stream = null;
  context = null;
  processor = null;
  chunks = [];
  sampleRate = TARGET_SAMPLE_RATE;
  recording = false;
  get active() {
    return this.recording;
  }
  async requestAccess() {
    this.assertAvailable();
    let stream = null;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } finally {
      stream?.getTracks().forEach((track) => track.stop());
    }
  }
  async start(onLevel) {
    if (this.recording) return;
    this.assertAvailable();
    this.cleanup();
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        autoGainControl: true,
        echoCancellation: true,
        noiseSuppression: true
      }
    });
    try {
      const context = new AudioContext({ latencyHint: "interactive" });
      if (context.state === "suspended") await context.resume();
      const source = context.createMediaStreamSource(this.stream);
      const processor = context.createScriptProcessor(4096, 1, 1);
      const maxSamples = context.sampleRate * MAX_CAPTURE_SECONDS;
      this.sampleRate = context.sampleRate;
      this.chunks = [];
      let collected = 0;
      processor.onaudioprocess = (event) => {
        const samples = event.inputBuffer.getChannelData(0);
        if (collected < maxSamples) {
          this.chunks.push(new Float32Array(samples));
          collected += samples.length;
        }
        if (onLevel) {
          let sum = 0;
          for (const value of samples) sum += value * value;
          onLevel(Math.min(1, Math.sqrt(sum / samples.length) * 7));
        }
      };
      source.connect(processor);
      processor.connect(context.destination);
      this.context = context;
      this.processor = processor;
      this.recording = true;
    } catch (error) {
      this.cleanup();
      throw error;
    }
  }
  async stop() {
    if (!this.recording) throw new Error("No recording is active");
    const chunks = this.chunks;
    const sampleRate = this.sampleRate;
    this.cleanup();
    const samples = downsample(concat(chunks), sampleRate, TARGET_SAMPLE_RATE);
    if (!samples.length) throw new Error("The recording is empty");
    const blob = new Blob([encodeWav(samples, TARGET_SAMPLE_RATE)]);
    return { audioBase64: await toBase64(blob), format: "wav" };
  }
  cancel() {
    this.cleanup();
  }
  assertAvailable() {
    if (!navigator.mediaDevices?.getUserMedia || typeof AudioContext === "undefined") {
      throw new Error("Audio capture is unavailable in this window.");
    }
  }
  cleanup() {
    this.recording = false;
    this.chunks = [];
    if (this.processor) {
      this.processor.onaudioprocess = null;
      this.processor.disconnect();
    }
    this.processor = null;
    if (this.context && this.context.state !== "closed") void this.context.close();
    this.context = null;
    this.stream?.getTracks().forEach((track) => track.stop());
    this.stream = null;
  }
}
function concat(chunks) {
  const merged = new Float32Array(chunks.reduce((total, chunk) => total + chunk.length, 0));
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.length;
  }
  return merged;
}
function downsample(samples, from, to) {
  if (from <= to || !samples.length) return samples;
  const ratio = from / to;
  const output = new Float32Array(Math.floor(samples.length / ratio));
  for (let index = 0; index < output.length; index += 1) {
    const position = index * ratio;
    const left = Math.floor(position);
    const right = Math.min(left + 1, samples.length - 1);
    output[index] = samples[left] + (samples[right] - samples[left]) * (position - left);
  }
  return output;
}
function encodeWav(samples, sampleRate) {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);
  const writeAscii = (offset, text) => {
    for (let index = 0; index < text.length; index += 1) {
      view.setUint8(offset + index, text.charCodeAt(index));
    }
  };
  writeAscii(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  writeAscii(8, "WAVE");
  writeAscii(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeAscii(36, "data");
  view.setUint32(40, samples.length * 2, true);
  for (let index = 0; index < samples.length; index += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[index]));
    view.setInt16(44 + index * 2, clamped < 0 ? clamped * 32768 : clamped * 32767, true);
  }
  return buffer;
}
function toBase64(blob) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Unable to read the recording"));
    reader.onload = () => resolve(String(reader.result ?? "").split(",").at(-1) ?? "");
    reader.readAsDataURL(blob);
  });
}
const speechCapture = new SpeechCapture();
function cloudConfigured() {
  return Boolean(convexUrl && nativeRuntimeAvailable());
}
async function loadConvex() {
  if (!convexUrl) throw new Error("Cloud sync is not configured");
  if (convex) return convex;
  convexLoad ??= Promise.all([
    import("convex/browser"),
    import("convex/server")
  ]).then(([browser, server]) => {
    convexApi = server.anyApi;
    convex = new browser.ConvexClient(convexUrl);
    return convex;
  }).catch((error) => {
    convex = null;
    convexApi = null;
    convexLoad = null;
    throw error;
  });
  return await convexLoad;
}
function startCloudAuth(onAuthenticated) {
  if (!convexUrl || !nativeRuntimeAvailable()) return false;
  const start = () => {
    void loadConvex()
      .then((client) => {
        client.setAuth(
          async ({ forceRefreshToken }) => await invoke("clerk_account_token", {
            forceRefresh: forceRefreshToken
          }),
          onAuthenticated
        );
      })
      .catch(() => onAuthenticated(false));
  };
  if (typeof window.requestIdleCallback === "function") {
    window.requestIdleCallback(start, { timeout: 1500 });
  } else {
    window.setTimeout(start, 500);
  }
  return true;
}
async function pushCloud(payload) {
  const client = await loadConvex();
  await client.mutation(convexApi.sync.upsertSnapshot, { payload });
}
async function pullCloud() {
  const client = await loadConvex();
  return await client.query(convexApi.sync.latestSnapshot, {});
}
const runtime = {
  invoke,
  listen,
  openDirectory: () => open({ directory: true, multiple: false, title: "Choose a workspace" }),
  openFiles: () => open({ directory: false, multiple: true, title: "Attach files" }),
  openUrl,
  setWindowTheme: (theme) => getCurrentWindow().setTheme(theme),
  mountTerminal,
  unmountTerminal,
  writeTerminal,
  exitTerminal,
  clearTerminal,
  forgetTerminal,
  requestMicrophoneAccess: () => speechCapture.requestAccess(),
  startAudioCapture: (onLevel) => speechCapture.start(onLevel),
  stopAudioCapture: () => speechCapture.stop(),
  cancelAudioCapture: () => speechCapture.cancel(),
  audioCaptureActive: () => speechCapture.active,
  playAudio: async (source) => {
    if (source) await new Audio(source).play();
  },
  copyText: (text) => navigator.clipboard.writeText(text),
  cloudConfigured,
  startCloudAuth,
  pushCloud,
  pullCloud
};
Object.assign(window, { __ONYX_RUNTIME__: runtime });
