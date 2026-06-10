import XCTest

@testable import KarukanIME

/// Integration tests driving a real karukan-imserver binary through
/// EngineProcess + EngineClient. Skipped when the Rust binary hasn't been
/// built (run `cargo build -p karukan-im --bin karukan-imserver` first;
/// `make test` does this automatically).
///
/// Only config-independent requests are exercised: the server loads the
/// user's config.toml, so anything involving conversion behavior is
/// covered by the Rust-side tests instead.
final class TransportTests: XCTestCase {
    static func serverBinaryPath() -> String? {
        // <repo>/karukan-macos/Tests/KarukanIMETests/TransportTests.swift
        let repoRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // KarukanIMETests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // karukan-macos
            .deletingLastPathComponent()  // repo root
        for profile in ["release", "debug"] {
            let candidate =
                repoRoot
                .appendingPathComponent("target/\(profile)/karukan-imserver").path
            if FileManager.default.fileExists(atPath: candidate) {
                return candidate
            }
        }
        return nil
    }

    private var process: EngineProcess!
    private var client: EngineClient!

    override func setUpWithError() throws {
        guard let path = Self.serverBinaryPath() else {
            throw XCTSkip("karukan-imserver not built")
        }
        process = EngineProcess(serverPath: path)
        client = EngineClient(serverProcess: process, autoInit: false)
        process.start()
        client.startReaderLoop()
    }

    override func tearDown() {
        process?.stop()
    }

    func testStatusRoundTrip() throws {
        let data = client.sendRequestSync(method: "status", params: [:], timeout: 5.0)
        let json = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: XCTUnwrap(data)) as? [String: Any])
        XCTAssertEqual(json["initialized"] as? Bool, false)
        XCTAssertEqual(json["state"] as? String, "empty")
    }

    func testEscapeInEmptyStateNotConsumed() throws {
        let key = EngineKeyEvent(keysym: 0xff1b, modifiers: KeyModifiers())
        let result = try XCTUnwrap(client.processKeySync(key))
        XCTAssertFalse(result.consumed)
    }

    func testUnknownMethodReturnsNil() {
        let data = client.sendRequestSync(method: "no_such_method", params: [:], timeout: 5.0)
        XCTAssertNil(data)
    }

    func testManySequentialRequests() throws {
        // The reader loop must keep request/response pairing intact.
        for _ in 0..<50 {
            let data = client.sendRequestSync(method: "status", params: [:], timeout: 5.0)
            XCTAssertNotNil(data)
        }
    }

    func testServerStopAndRestartRecovers() throws {
        // restart() waits for the old process off the main thread and
        // completes via onRestart on the main queue; wait(for:) pumps the
        // run loop so that completion can fire.
        let restarted = expectation(description: "server restarted")
        let previousOnRestart = process.onRestart
        process.onRestart = {
            previousOnRestart?()
            restarted.fulfill()
        }
        process.restart()
        wait(for: [restarted], timeout: 5.0)
        let data = client.sendRequestSync(method: "status", params: [:], timeout: 5.0)
        XCTAssertNotNil(data)
    }
}
