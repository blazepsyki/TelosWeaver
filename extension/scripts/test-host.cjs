// Run only in an isolated Extension Development Host (Spec §19.5.8).
const fs = require("node:fs/promises");
const path = require("node:path");
const os = require("node:os");
const { spawn } = require("node:child_process");

(async () => {
  if (
    !process.env.TELOS_VSCODE_EXECUTABLE ||
    !process.env.TELOS_CORE_EXECUTABLE
  )
    throw new Error(
      "TELOS_VSCODE_EXECUTABLE and TELOS_CORE_EXECUTABLE are required",
    );
  const base = await fs.mkdtemp(path.join(os.tmpdir(), "telos-ext-host-"));
  const project = path.join(base, "project");
  const state = path.join(base, "state");
  await fs.mkdir(project);
  await fs.mkdir(state);
  const env = {
    ...process.env,
    TELOS_HOST_RESULTS: path.join(base, "result.json"),
    LOCALAPPDATA: state,
    XDG_STATE_HOME: state,
    XDG_RUNTIME_DIR: state,
  };
  delete env.ELECTRON_RUN_AS_NODE;
  delete env.VSCODE_IPC_HOOK_CLI;
  const args = [
    "--new-window",
    "--user-data-dir",
    path.join(base, "profile"),
    "--extensions-dir",
    path.join(base, "extensions"),
    "--disable-extensions",
    "--disable-workspace-trust",
    "--skip-welcome",
    "--skip-release-notes",
    "--skip-add-to-recently-opened",
    "--disable-updates",
    "--disable-telemetry",
    `--extensionDevelopmentPath=${path.resolve(__dirname, "..")}`,
    `--extensionTestsPath=${path.resolve(__dirname, "../dist/test/host.js")}`,
    project,
  ];
  const child = spawn(process.env.TELOS_VSCODE_EXECUTABLE, args, {
    env,
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  for (const stream of [child.stdout, child.stderr])
    stream.on("data", (data) => {
      output = (output + data).slice(-16000);
    });
  const code = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error(`Extension Host timeout; artifact ${base}`));
    }, 90000);
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      resolve(code);
    });
  });
  console.log(`Extension Host exit=${code}; artifact=${base}`);
  let result;
  try {
    result = JSON.parse(await fs.readFile(env.TELOS_HOST_RESULTS, "utf8"));
  } catch {
    console.log(output);
    throw new Error("Extension Host result missing");
  }
  if (result.passed && Number.isInteger(result.corePid) && result.corePid > 0) {
    try {
      process.kill(result.corePid, 0);
      result.coreSurvivedHostExit = true;
    } finally {
      try {
        process.kill(result.corePid);
      } catch {}
    }
  }
  await fs.writeFile(env.TELOS_HOST_RESULTS, JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result, null, 2));
  if (code !== 0) {
    console.log(output);
    throw new Error("Extension Host failed");
  }
  // Keep the isolated result/profile directory as local evidence, never reuse a user profile.
})().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
