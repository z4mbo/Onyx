(() => {
  const MAX_DETAIL_LENGTH = 2_048;
  const root = document.getElementById("root");
  let failed = false;

  const describe = (value) => {
    if (value instanceof Error) {
      return `${value.name}: ${value.message}\n${value.stack || ""}`.slice(
        0,
        MAX_DETAIL_LENGTH,
      );
    }
    if (typeof value === "string") {
      return value.slice(0, MAX_DETAIL_LENGTH);
    }
    try {
      return JSON.stringify(value).slice(0, MAX_DETAIL_LENGTH);
    } catch {
      return String(value).slice(0, MAX_DETAIL_LENGTH);
    }
  };

  const showFailure = (summary, detail) => {
    // This watchdog owns only the pre-mount phase. Once Leptos has mounted,
    // background command failures belong to the Rust UI and must never replace
    // the entire application shell.
    if (failed || root?.dataset.onyxMounted === "true") return;
    failed = true;
    if (!root) return;

    const status = document.createElement("main");
    status.className = "onyx-boot-status onyx-boot-status--error";
    status.setAttribute("role", "alert");
    const panel = document.createElement("div");
    const heading = document.createElement("strong");
    heading.textContent = "Onyx could not start";
    const message = document.createElement("p");
    message.textContent = summary;
    const diagnostic = document.createElement("code");
    diagnostic.textContent = describe(detail) || "No diagnostic was provided.";
    panel.append(heading, message, diagnostic);
    status.append(panel);
    root.replaceChildren(status);
  };

  const nativeConsoleError = console.error.bind(console);
  console.error = (...values) => {
    nativeConsoleError(...values);
    const detail = values.map(describe).join("\n");
    if (
      detail.includes("panicked at")
      || detail.includes("RuntimeError: unreachable")
      || detail.includes("wasm-function")
    ) {
      showFailure("The Rust interface stopped unexpectedly.", detail);
    }
  };

  window.addEventListener(
    "error",
    (event) => {
      if (!(event instanceof ErrorEvent)) return;
      showFailure(
        "The Rust interface encountered a JavaScript error.",
        event.error || `${event.message} (${event.filename}:${event.lineno})`,
      );
    },
    true,
  );

  window.addEventListener("unhandledrejection", (event) => {
    showFailure(
      "The Rust interface could not finish loading.",
      event.reason || "Unhandled promise rejection.",
    );
  });

  document.addEventListener("securitypolicyviolation", (event) => {
    showFailure(
      "The app security policy blocked a required frontend resource.",
      `${event.effectiveDirective}: ${event.blockedURI || "inline resource"}`,
    );
  });

  window.setTimeout(() => {
    if (root?.dataset.onyxMounted !== "true") {
      showFailure(
        "The Rust interface did not mount within 8 seconds.",
        `page=${window.location.href}`,
      );
    }
  }, 8_000);
})();
