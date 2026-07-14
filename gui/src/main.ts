import { createApp } from "vue";
import App from "./App.vue";
import { initCore } from "./core-bridge";
import "./style.css";

initCore()
  .then(() => createApp(App).mount("#app"))
  .catch((e: unknown) => {
    // wasm init failure: don't let the whole screen be non-functional — show the cause
    // (never make it "silently stop working" §7)
    const el = document.getElementById("app");
    if (el) {
      const pre = document.createElement("pre");
      pre.className = "fatal";
      pre.textContent = `Failed to initialize the core wasm:\n${String(e)}\n\nRun npm run build:core in gui/ to rebuild the assets`;
      el.replaceChildren(pre);
    }
  });
