import Cocoa

/// Manages the bundled karukan-imserver child process: spawn, crash restart
/// with exponential backoff, and clean shutdown (closing stdin lets the
/// server save its learning cache before exiting).
class EngineProcess {
    private var process: Process?
    private(set) var stdinPipe: Pipe?
    private(set) var stdoutPipe: Pipe?

    private var restartCount = 0
    private var shouldRestart = true
    private var pendingRestart: DispatchWorkItem?
    private var terminationObserver: NSObjectProtocol?

    /// Called after the server (re)starts; the JSON-RPC client uses this to
    /// re-attach its reader loop and re-send `init`.
    var onRestart: (() -> Void)?

    private let serverPathOverride: String?

    /// `serverPath` overrides the bundled binary location (used by tests
    /// and `swift run` development).
    init(serverPath: String? = nil) {
        self.serverPathOverride = serverPath
    }

    private func serverPath() -> String {
        if let override = serverPathOverride {
            return override
        }
        // Development override: run the IME from `swift run` against a
        // locally built server without assembling the bundle.
        if let override = ProcessInfo.processInfo.environment["KARUKAN_IMSERVER"] {
            return override
        }
        return Bundle.main.bundlePath + "/Contents/MacOS/karukan-imserver"
    }

    func start() {
        if let existing = process, existing.isRunning {
            NSLog("KarukanIME: karukan-imserver already running (pid=\(existing.processIdentifier))")
            return
        }

        let path = serverPath()
        guard FileManager.default.fileExists(atPath: path) else {
            NSLog("KarukanIME: karukan-imserver not found at \(path)")
            return
        }

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: path)
        var env = ProcessInfo.processInfo.environment
        if env["RUST_LOG"] == nil {
            env["RUST_LOG"] = "info"
        }
        proc.environment = env

        let stdin = Pipe()
        let stdout = Pipe()
        proc.standardInput = stdin
        proc.standardOutput = stdout
        // stderr is inherited: server logs land in the IME's log file.

        proc.terminationHandler = { [weak self] terminatedProcess in
            let status = terminatedProcess.terminationStatus
            NSLog("KarukanIME: karukan-imserver terminated with status \(status)")
            DispatchQueue.main.async { self?.handleTermination(of: terminatedProcess) }
        }

        do {
            try proc.run()
            NSLog("KarukanIME: karukan-imserver started (pid=\(proc.processIdentifier))")
            self.process = proc
            self.stdinPipe = stdin
            self.stdoutPipe = stdout
        } catch {
            NSLog("KarukanIME: failed to start karukan-imserver: \(error)")
        }

        if terminationObserver == nil {
            terminationObserver = NotificationCenter.default.addObserver(
                forName: NSApplication.willTerminateNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.stop()
            }
        }
    }

    // Must run on the main queue. The `process === terminatedProcess` check
    // prevents a double start when restart() already replaced the process.
    private func handleTermination(of terminatedProcess: Process) {
        guard process === terminatedProcess, shouldRestart else { return }

        let delay = min(pow(2.0, Double(restartCount)), 15.0)
        restartCount += 1
        NSLog("KarukanIME: restarting karukan-imserver in \(delay)s (attempt \(restartCount))")

        let workItem = DispatchWorkItem { [weak self] in
            guard let self = self else { return }
            self.pendingRestart = nil
            self.start()
            self.onRestart?()
        }
        pendingRestart = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: workItem)
    }

    /// Mark the server healthy after a successful round-trip so the next
    /// crash starts backoff from the beginning.
    func resetBackoff() {
        restartCount = 0
    }

    /// Forced restart (e.g. wake from sleep, which can invalidate pipes).
    func restart() {
        pendingRestart?.cancel()
        pendingRestart = nil
        shouldRestart = false
        terminateAndWait()
        shouldRestart = true
        restartCount = 0
        start()
        onRestart?()
    }

    func stop() {
        pendingRestart?.cancel()
        pendingRestart = nil
        shouldRestart = false
        terminateAndWait()
    }

    private func terminateAndWait() {
        guard let proc = process, proc.isRunning else {
            process = nil
            stdinPipe = nil
            stdoutPipe = nil
            return
        }
        // Closing stdin sends EOF; the server saves its learning cache and
        // exits on its own. SIGTERM only as a fallback.
        try? stdinPipe?.fileHandleForWriting.close()
        let deadline = Date().addingTimeInterval(2.0)
        while proc.isRunning && Date() < deadline {
            usleep(50_000)
        }
        if proc.isRunning {
            NSLog("KarukanIME: karukan-imserver did not exit on EOF, terminating")
            proc.terminate()
            proc.waitUntilExit()
        }
        process = nil
        stdinPipe = nil
        stdoutPipe = nil
    }
}
