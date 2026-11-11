const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const rowsEl = document.getElementById("rows");

function span(className, text) {
  const el = document.createElement("span");
  el.className = className;
  el.textContent = text;
  return el;
}

function render(view) {
  rowsEl.replaceChildren(
    ...view.rows.map((row) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "row";
      button.title = `${row.label} を開く`;
      button.append(span("label", row.label));
      if (row.note) button.append(span("note", row.note));
      button.append(span("count", row.count));
      button.addEventListener("click", () => invoke("open_app", { target: row.app }));
      return button;
    }),
  );
}

const zukunftEl = document.getElementById("zukunft");
zukunftEl.addEventListener("click", () =>
  invoke("open_zukunft").then(
    () => {
      zukunftEl.title = "Zukunft を開く";
      zukunftEl.classList.remove("failed");
    },
    (error) => {
      zukunftEl.title = String(error);
      zukunftEl.classList.add("failed");
    },
  ),
);

document.getElementById("dismiss").addEventListener("click", () => invoke("dismiss"));
document.addEventListener("contextmenu", (event) => event.preventDefault());

listen("view-updated", (event) => render(event.payload));
invoke("get_view").then(render);
