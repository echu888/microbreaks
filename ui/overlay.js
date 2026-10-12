const { invoke } = window.__TAURI__.core;

const ESC_WINDOW_MS = 1000;

function el(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

async function main() {
  const view = await invoke("current_break");
  if (!view) return;
  document.body.style.background = `rgba(0, 0, 0, ${view.opacity})`;
  // Secondary monitors are only dimmed.
  if (view.panel) mountPanel(view);
}

function mountPanel({ spec, input_grace_ms }) {
  const isEye = spec.kind === "eye";
  let graceOver = false;
  let countdownOver = !isEye;
  let submitted = false;
  let lastEsc = -Infinity;

  const hint = el("div", { className: "hint" });
  const done = el("button", { id: "done", textContent: "Done", disabled: true });
  const skip = el("button", { id: "skip", textContent: "Skip" });
  const body = el("div");
  const inputs = [];

  if (isEye) {
    const count = el("div", { className: "sub" }, String(spec.duration_secs));
    body.append(count);
    let left = spec.duration_secs;
    const timer = setInterval(() => {
      left -= 1;
      count.textContent = String(Math.max(left, 0));
      if (left <= 0) {
        clearInterval(timer);
        countdownOver = true;
        refresh();
      }
    }, 1000);
  } else {
    for (const ex of spec.exercises) {
      const input = el("input", { type: "number", min: "0", value: String(ex.reps), disabled: true });
      const adjust = (d) => {
        input.value = String(Math.max(0, (parseInt(input.value) || 0) + d));
      };
      const minus = el("button", { textContent: "−", tabIndex: -1, disabled: true });
      const plus = el("button", { textContent: "+", tabIndex: -1, disabled: true });
      minus.onclick = () => adjust(-1);
      plus.onclick = () => adjust(1);
      inputs.push(input);
      body.append(el("div", { className: "row" }, el("span", { className: "name" }, ex.name), minus, input, plus));
    }
  }

  function refresh() {
    done.disabled = !(graceOver && countdownOver);
  }

  function finish(command) {
    if (submitted) return;
    submitted = true;
    const reps = inputs.map((i) => Math.max(0, parseInt(i.value) || 0));
    invoke(command, command === "break_done" ? { id: spec.id, reps } : { id: spec.id });
  }

  done.onclick = () => finish("break_done");
  skip.onclick = () => finish("break_skip");

  // Capture phase: swallow every key during the input grace period, so in-flight typing
  // cannot acknowledge the break or edit reps.
  window.addEventListener(
    "keydown",
    (e) => {
      if (!graceOver) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        if (!done.disabled) finish("break_done");
      } else if (e.key === "Escape") {
        e.preventDefault();
        if (e.repeat) return;
        const now = performance.now();
        if (now - lastEsc <= ESC_WINDOW_MS) {
          finish("break_skip");
        } else {
          lastEsc = now;
          hint.textContent = "Press Esc again to skip";
          setTimeout(() => {
            if (performance.now() - lastEsc >= ESC_WINDOW_MS) hint.textContent = "";
          }, ESC_WINDOW_MS);
        }
      }
    },
    true,
  );

  setTimeout(() => {
    graceOver = true;
    for (const b of body.querySelectorAll(".row button")) b.disabled = false;
    for (const i of inputs) i.disabled = false;
    inputs[0]?.focus();
    inputs[0]?.select();
    refresh();
  }, input_grace_ms);

  const title = el("h1", {}, isEye ? "Look 20 ft away" : "Movement time");
  document
    .getElementById("app")
    .append(el("div", { className: "panel" }, title, body, el("div", { className: "actions" }, done, skip), hint));
}

main();
