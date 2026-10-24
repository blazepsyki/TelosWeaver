// Local socket fixtures only; no provider or model calls (Spec §19.5.7).
const net = require("node:net");
const os = require("node:os");
const path = require("node:path");
const fs = require("node:fs/promises");
const crypto = require("node:crypto");
exports.endpoint = () =>
  process.platform === "win32"
    ? `\\\\.\\pipe\\telos-ext-test-${crypto.randomUUID()}`
    : path.join(os.tmpdir(), `telos-${crypto.randomUUID()}.sock`);
exports.server = async (handler, address = exports.endpoint()) => {
  const sockets = new Set();
  const server = net.createServer((socket) => {
    sockets.add(socket);
    socket.on("error", () => {});
    socket.on("close", () => sockets.delete(socket));
    let buffer = "";
    socket.on("data", (chunk) => {
      buffer += chunk.toString();
      let index;
      while ((index = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, index);
        buffer = buffer.slice(index + 1);
        handler(JSON.parse(line), socket);
      }
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(address, resolve);
  });
  return {
    address,
    close: async () => {
      for (const socket of sockets) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
    },
  };
};
exports.reply = (socket, request, result) =>
  socket.write(
    JSON.stringify({
      protocolVersion: "0.2",
      requestId: request.requestId,
      result,
    }) + "\n",
  );
exports.notify = (socket, method, params) =>
  socket.write(
    JSON.stringify({ protocolVersion: "0.2", method, params }) + "\n",
  );
exports.environment = async () => {
  const base = await fs.mkdtemp(path.join(os.tmpdir(), "telos-ext-"));
  const root = path.join(base, "project");
  const state = path.join(base, "state");
  await fs.mkdir(root);
  await fs.mkdir(state);
  const env = {
    ...process.env,
    LOCALAPPDATA: state,
    XDG_STATE_HOME: state,
    XDG_RUNTIME_DIR: state,
  };
  const location = await require("../dist/locations").locations(root, env);
  await fs.mkdir(path.dirname(location.tokenPath), { recursive: true });
  const token = crypto.randomBytes(32).toString("hex");
  await fs.writeFile(location.tokenPath, token);
  if (process.platform !== "win32")
    await fs.mkdir(path.dirname(location.endpoint), { recursive: true });
  return {
    base,
    root,
    env,
    location,
    token,
    close: () => fs.rm(base, { recursive: true, force: true }),
  };
};
exports.hello = {
  serverVersion: "0.0.0",
  protocolVersion: "0.2",
  projectId: "fixture-project",
  hostId: "fixture-host",
  connectionId: "fixture-connection",
};
exports.health = {
  serverVersion: "0.0.0",
  status: "degraded",
  components: [{ name: "persistence", status: "unavailable" }],
};
