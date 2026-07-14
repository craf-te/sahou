import { SahouRejected, connect } from "sahou/browser";

const out = document.getElementById("out");
const descriptor = await (await fetch("/gen/descriptor.json")).text();
try {
  const node = await connect(descriptor, { node: "visuals" });
  node.onReject((conn, diags) => {
    out.textContent = `boundary NO on ${conn}: ` + diags.map((d) => `[${d.code}] ${d.message}`).join("; ");
  });
  await node.subscribe("touch", (p) => {
    out.textContent = JSON.stringify(p, null, 2);
  });
} catch (e) {
  // link not running → the link_unavailable NO with startup instructions is shown on screen verbatim (never a silent blank page)
  out.textContent = e instanceof SahouRejected ? e.message : String(e);
}
