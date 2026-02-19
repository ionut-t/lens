export default class LensReporter {
  constructor() {
    this._currentFile = null;
    this._originalWrite = process.stdout.write.bind(process.stdout);

    // Intercept stdout.write to capture console.log from tests.
    // With --disableConsoleIntercept, console.log goes directly to stdout.
    // We wrap non-NDJSON lines with the current file context.
    process.stdout.write = (chunk, encoding, callback) => {
      const str = typeof chunk === "string" ? chunk : chunk.toString();
      const trimmed = str.trim();

      // Our own NDJSON lines start with '{' — pass through
      if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
        return this._originalWrite(chunk, encoding, callback);
      }

      // Non-JSON output while a file is running = console.log from test code
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

  onTestRunStart(specifications) {
    this._startTime = Date.now();
    this._emit({ type: "run-started", total: specifications.length });
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
    // Emit failures for unhandled errors
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

    let passed = 0;
    let failed = 0;
    let skipped = 0;
    let total = 0;
    let duration = 0;

    for (const mod of modules) {
      for (const test of mod.children.allTests()) {
        total += 1;
        const r = test.result();
        if (r.state === "passed") passed += 1;
        else if (r.state === "failed") failed += 1;
        else if (r.state === "skipped") skipped += 1;
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
