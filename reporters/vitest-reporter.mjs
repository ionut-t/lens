export default class LensReporter {
  constructor() {
    this._currentFile = null;
    this._originalWrite = process.stdout.write.bind(process.stdout);

    // Intercept stdout.write to capture console.log from tests.
    process.stdout.write = (chunk, encoding, callback) => {
      const str = typeof chunk === "string" ? chunk : chunk.toString();
      const trimmed = str.trim();

      if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
        return this._originalWrite(chunk, encoding, callback);
      }

      if (this._currentFile && trimmed) {
        const event =
          JSON.stringify({
            type: "console-log",
            file: this._currentFile,
            content: trimmed,
          }) + "\n";
        return this._originalWrite(event, encoding, callback);
      }

      return this._originalWrite(chunk, encoding, callback);
    };
  }

  onInit(vitest) {
    this.ctx = vitest;
    this._emit({ type: "output", line: "[REPORTER] Initialized" });

    process.stdout.on("error", () => process.exit(0));

    // Prevent the stdin listener from keeping the process alive in non-watch (vitest run) mode.
    process.stdin.unref();

    process.stdin.on("data", async (data) => {
      const lines = data.toString().split("\n");
      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed.startsWith("LENS_RUN:")) {
          try {
            const cmd = JSON.parse(trimmed.substring(9));
            this._emit({
              type: "output",
              line: "[REPORTER] Command: " + JSON.stringify(cmd),
            });
            await this._handleCommand(cmd);
          } catch (e) {
            this._emit({
              type: "error",
              message:
                "Reporter failed to handle command: " +
                e.message +
                "\n" +
                e.stack,
            });
          }
        }
      }
    });
  }

  async _handleCommand(cmd) {
    if (!this.ctx) {
      this._emit({ type: "error", message: "Reporter context not available" });
      return;
    }

    // Clear filters by default
    this.ctx.config.testNamePattern = undefined;

    if (cmd.type === "run-all") {
      await this.ctx.rerunFiles();
    } else if (cmd.type === "run-file" || cmd.type === "run-test") {
      if (cmd.type === "run-test") {
        // Escape special regex characters in the test name
        const escapedName = cmd.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        this.ctx.config.testNamePattern = new RegExp(escapedName);
      }

      // Try to find the file in the projects to ensure it's a known test file.
      // If we can't find it precisely, we just pass the path and let Vitest decide.
      let files = [cmd.file];

      try {
        const allTestFiles = this.ctx.projects.flatMap((p) => {
          // Try different ways to get files depending on vitest version
          if (p.specifier?.files) return Array.from(p.specifier.files);
          if (p.testFiles) return p.testFiles;
          return [];
        });

        if (allTestFiles.length > 0) {
          const found = allTestFiles.find(
            (f) => f === cmd.file || f.endsWith(cmd.file),
          );
          if (found) {
            files = [found];
          }
        }
      } catch (e) {
        this._emit({
          type: "output",
          line: "[REPORTER] Note: Precise file lookup failed, using path directly",
        });
      }

      this._emit({ type: "output", line: `[REPORTER] Rerunning: ${files[0]}` });
      await this.ctx.rerunFiles(files);
    }
  }

  onTestRunStart(specifications) {
    this._startTime = Date.now();
    this._emit({ type: "run-started", total: specifications.length });
  }

  onTestModuleCollected(module) {
    let count = 0;
    for (const _test of module.children.allTests()) {
      count += 1;
    }
    this._emit({ type: "tests-collected", file: module.moduleId, count });
  }

  onTestModuleStart(module) {
    this._currentFile = module.moduleId;
    this._emit({ type: "file-started", file: module.moduleId });
  }

  onTestSuiteResult(suite) {
    const loc = suite.location;
    if (loc) {
      this._emit({
        type: "suite-location",
        file: suite.module.moduleId,
        name: suite.fullName,
        location: { line: loc.line, column: loc.column },
      });
    }
  }

  onTestCaseStart(testCase) {
    this._emit({
      type: "test-started",
      file: testCase.module.moduleId,
      name: testCase.fullName,
    });
  }

  onTestCaseResult(testCase) {
    const result = testCase.result();
    const diag = testCase.diagnostic();

    const loc = testCase.location;
    const event = {
      type: "test-finished",
      file: testCase.module.moduleId,
      name: testCase.fullName,
      state: result.state,
      duration: diag?.duration,
      location: loc ? { line: loc.line, column: loc.column } : undefined,
    };

    if (result.state === "failed" && result.errors.length > 0) {
      const err = result.errors[0];
      event.error = {
        message: err.message ?? "",
        expected: err.expected,
        actual: err.actual,
        diff: err.diff,
        stack: err.stack,
      };
    }

    this._emit(event);
  }

  onTestModuleEnd(module) {
    this._emit({ type: "file-finished", file: module.moduleId });
    this._currentFile = null;
  }

  onTestRunEnd(modules, unhandledErrors, reason) {
    for (const err of unhandledErrors) {
      let message = "Unknown error";
      let stack;
      try {
        if (err && typeof err === "object" && "message" in err) {
          message = String(err.message);
          stack = err.stack ? String(err.stack) : undefined;
        } else {
          message = JSON.stringify(err);
        }
      } catch {
        message = "Unserializable error";
      }
      this._emit({
        type: "test-finished",
        file: "(unhandled)",
        name: "(unhandled error)",
        state: "fail",
        duration: 0,
        error: { message, stack },
      });
    }

    let total = 0;
    let passed = 0;
    let failed = 0;
    let skipped = 0;

    for (const mod of modules) {
      for (const test of mod.children.allTests()) {
        total += 1;
        const r = test.result();
        if (r.state === "passed") passed += 1;
        else if (r.state === "failed") failed += 1;
        else skipped += 1;
      }
    }

    this._emit({
      type: "run-finished",
      total,
      passed,
      failed,
      skipped,
      duration: Date.now() - (this._startTime || Date.now()),
    });
  }

  _emit(event) {
    this._originalWrite(JSON.stringify(event) + "\n");
  }
}
