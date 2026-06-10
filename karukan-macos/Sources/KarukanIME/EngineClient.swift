import Foundation

/// Newline-delimited JSON-RPC 2.0 client for karukan-imserver.
///
/// Requests are written to the child's stdin; a dedicated reader queue
/// splits stdout on 0x0A and dispatches responses to pending completions.
/// Key processing uses the synchronous API (the IMK `handle` callback must
/// answer "consumed?" synchronously, the same trade-off Mozc makes); slow
/// or fire-and-forget calls use the async API.
class EngineClient {
    private let serverProcess: EngineProcess
    private var nextID = 1
    private let requestQueue = DispatchQueue(label: "dev.togatoga.karukan.jsonrpc.request")

    private let lock = NSLock()
    private var pendingRequests: [Int: (Data?) -> Void] = [:]

    /// `autoInit` re-sends `init` whenever the server (re)starts. Tests
    /// disable it to avoid loading models.
    init(serverProcess: EngineProcess, autoInit: Bool = true) {
        self.serverProcess = serverProcess
        self.serverProcess.onRestart = { [weak self] in
            self?.startReaderLoop()
            if autoInit {
                self?.initAsync()
            }
        }
    }

    // MARK: - Engine methods

    func initAsync() {
        sendRequest(method: "init", params: [:]) { [weak self] data in
            guard let self else { return }
            guard let data,
                let result = try? makeProtocolDecoder().decode(InitResult.self, from: data)
            else {
                NSLog("KarukanIME: engine init failed")
                return
            }
            self.serverProcess.resetBackoff()
            NSLog(
                "KarukanIME: engine initialized (protocol v\(result.protocolVersion), model=\(result.modelName))"
            )
        }
    }

    func processKeySync(_ key: EngineKeyEvent, isRelease: Bool = false) -> KeyResult? {
        let params: [String: Any] = [
            "keysym": key.keysym,
            "modifiers": key.modifiers.jsonObject,
            "is_release": isRelease,
        ]
        return keyResultSync(method: "process_key", params: params, timeout: 3.0)
    }

    func commitSync() -> KeyResult? {
        keyResultSync(method: "commit", params: [:], timeout: 1.0)
    }

    func saveLearningAsync() {
        sendRequest(method: "save_learning", params: [:]) { _ in }
    }

    func setSurroundingTextAsync(text: String, cursorPos: Int) {
        sendRequest(
            method: "set_surrounding_text",
            params: ["text": text, "cursor_pos": cursorPos]
        ) { _ in }
    }

    private func keyResultSync(method: String, params: [String: Any], timeout: TimeInterval)
        -> KeyResult?
    {
        guard let data = sendRequestSync(method: method, params: params, timeout: timeout) else {
            return nil
        }
        do {
            return try makeProtocolDecoder().decode(KeyResult.self, from: data)
        } catch {
            NSLog("KarukanIME: failed to decode \(method) result: \(error)")
            return nil
        }
    }

    // MARK: - JSON-RPC transport

    func startReaderLoop() {
        guard let stdout = serverProcess.stdoutPipe else { return }

        let queue = DispatchQueue(label: "dev.togatoga.karukan.jsonrpc.reader")
        queue.async { [weak self] in
            let handle = stdout.fileHandleForReading
            var buffer = Data()

            while true {
                let chunk = handle.availableData
                if chunk.isEmpty {
                    // EOF: server terminated
                    self?.failAllPending()
                    break
                }
                buffer.append(chunk)

                while let newlineRange = buffer.range(of: Data([0x0A])) {
                    let lineData = buffer.subdata(in: buffer.startIndex..<newlineRange.lowerBound)
                    buffer.removeSubrange(buffer.startIndex...newlineRange.lowerBound)
                    guard !lineData.isEmpty else { continue }
                    self?.handleResponse(lineData)
                }
            }
        }
    }

    @discardableResult
    func sendRequest(
        method: String, params: [String: Any], completion: @escaping (Data?) -> Void
    ) -> Int {
        lock.lock()
        let id = nextID
        nextID += 1
        pendingRequests[id] = completion
        lock.unlock()

        let request: [String: Any] = [
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        ]

        requestQueue.async { [weak self] in
            guard let self,
                let stdin = self.serverProcess.stdinPipe,
                var data = try? JSONSerialization.data(withJSONObject: request)
            else {
                self?.takePending(id: id)?(nil)
                return
            }
            data.append(0x0A)
            do {
                try stdin.fileHandleForWriting.write(contentsOf: data)
            } catch {
                NSLog("KarukanIME: failed to write request: \(error)")
                self.takePending(id: id)?(nil)
            }
        }
        return id
    }

    func sendRequestSync(method: String, params: [String: Any], timeout: TimeInterval) -> Data? {
        let semaphore = DispatchSemaphore(value: 0)
        var result: Data?
        let id = sendRequest(method: method, params: params) { data in
            result = data
            semaphore.signal()
        }
        if semaphore.wait(timeout: .now() + timeout) == .timedOut {
            NSLog("KarukanIME: \(method) timed out after \(timeout)s")
            takePending(id: id)?(nil)
            return nil
        }
        return result
    }

    private func handleResponse(_ lineData: Data) {
        guard
            let json = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any]
        else {
            NSLog("KarukanIME: unparsable response line")
            return
        }
        guard let id = json["id"] as? Int else {
            // id:null happens only for parse errors on our side; log and drop.
            NSLog("KarukanIME: response without id: \(json)")
            return
        }
        if let error = json["error"] as? [String: Any] {
            NSLog("KarukanIME: engine error for request \(id): \(error)")
            takePending(id: id)?(nil)
            return
        }
        guard let result = json["result"],
            let data = try? JSONSerialization.data(withJSONObject: result)
        else {
            takePending(id: id)?(nil)
            return
        }
        takePending(id: id)?(data)
    }

    private func takePending(id: Int) -> ((Data?) -> Void)? {
        lock.lock()
        defer { lock.unlock() }
        return pendingRequests.removeValue(forKey: id)
    }

    private func failAllPending() {
        lock.lock()
        let pending = pendingRequests
        pendingRequests.removeAll()
        lock.unlock()
        for (_, completion) in pending {
            completion(nil)
        }
    }
}
