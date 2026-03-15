import Foundation
import ScreenCaptureKit
import CoreMedia
import CoreAudio

// Callback invoked on each audio chunk: (samples, count, sample_rate_hz)
public typealias AudioChunkCallback = @convention(c) (
    UnsafePointer<Float32>?,
    Int32,
    Float64
) -> Void

// Module-level state — only one loopback session at a time.
private enum LoopbackBackend {
    case none
    case screenCapture(stream: SCStream, delegate: LoopbackDelegate)
    case processTap(tap: Any)  // ProcessTapLoopback, type-erased for availability
}
private var gBackend: LoopbackBackend = .none

// MARK: - Public FFI entry points

/// Start capturing system audio. Non-blocking; audio arrives via `callback`.
/// Returns true if setup was initiated successfully.
///
/// Strategy: try AudioHardwareCreateProcessTap (macOS 14.2+) first — this
/// shows the yellow mic indicator instead of the purple screen-sharing one.
/// Falls back to ScreenCaptureKit on older macOS.
@_silgen_name("aura_loopback_start")
public func auraLoopbackStart(callback: @escaping AudioChunkCallback) -> Bool {
    // Try process tap first (macOS 14.2+)
    if #available(macOS 14.2, *) {
        let tap = ProcessTapLoopback(callback: callback)
        if tap.start() {
            gBackend = .processTap(tap: tap)
            NSLog("[AuraAudio] Using ProcessTap backend (no screen sharing indicator)")
            return true
        }
        NSLog("[AuraAudio] ProcessTap failed, falling back to ScreenCaptureKit")
    }

    // Fallback: ScreenCaptureKit
    return startScreenCapture(callback: callback)
}

/// Stop the loopback stream. Synchronous from Rust's perspective.
@_silgen_name("aura_loopback_stop")
public func auraLoopbackStop() {
    switch gBackend {
    case .processTap(let tap):
        if #available(macOS 14.2, *) {
            (tap as! ProcessTapLoopback).stop()
        }
    case .screenCapture(let stream, _):
        stream.stopCapture { _ in }
        NSLog("[AuraAudio] ScreenCaptureKit loopback stopped")
    case .none:
        break
    }
    gBackend = .none
}

// MARK: - ScreenCaptureKit fallback

private func startScreenCapture(callback: @escaping AudioChunkCallback) -> Bool {
    var success = false
    let semaphore = DispatchSemaphore(value: 0)

    SCShareableContent.getWithCompletionHandler { content, error in
        defer { semaphore.signal() }

        guard let content = content else {
            NSLog("[AuraAudio] getWithCompletionHandler failed: %@",
                  error?.localizedDescription ?? "unknown")
            return
        }
        guard let display = content.displays.first else {
            NSLog("[AuraAudio] No display found for loopback capture")
            return
        }

        let filter = SCContentFilter(
            display: display,
            excludingApplications: [],
            exceptingWindows: []
        )

        let config = SCStreamConfiguration()
        config.capturesAudio        = true
        config.sampleRate           = 44100   // Rust resampler handles → 16kHz
        config.channelCount         = 1       // mono from SCStream directly
        config.excludesCurrentProcessAudio = true

        let delegate = LoopbackDelegate(callback: callback)

        let stream = SCStream(filter: filter, configuration: config, delegate: delegate)
        do {
            try stream.addStreamOutput(
                delegate,
                type: .audio,
                sampleHandlerQueue: .global(qos: .userInteractive)
            )
        } catch {
            NSLog("[AuraAudio] addStreamOutput failed: %@", error.localizedDescription)
            return
        }

        stream.startCapture { error in
            if let error = error {
                NSLog("[AuraAudio] startCapture failed: %@", error.localizedDescription)
            } else {
                NSLog("[AuraAudio] ScreenCaptureKit loopback started (44100 Hz mono)")
            }
        }
        gBackend = .screenCapture(stream: stream, delegate: delegate)
        success = true
    }

    semaphore.wait()
    return success
}

// MARK: - SCStreamOutput delegate (ScreenCaptureKit fallback)

private final class LoopbackDelegate: NSObject, SCStreamOutput, SCStreamDelegate {
    private let callback: AudioChunkCallback

    init(callback: @escaping AudioChunkCallback) {
        self.callback = callback
    }

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of type: SCStreamOutputType
    ) {
        guard type == .audio else { return }

        // Grab the sample rate from the format description
        guard
            let formatDesc = CMSampleBufferGetFormatDescription(sampleBuffer),
            let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc)?.pointee
        else { return }

        let sampleRate = asbd.mSampleRate

        // Extract raw bytes via CMBlockBuffer — mono Float32, contiguous.
        guard let blockBuf = CMSampleBufferGetDataBuffer(sampleBuffer) else { return }

        var length: Int = 0
        var dataPointer: UnsafeMutablePointer<Int8>? = nil
        let status = CMBlockBufferGetDataPointer(
            blockBuf,
            atOffset:       0,
            lengthAtOffsetOut: nil,
            totalLengthOut: &length,
            dataPointerOut: &dataPointer
        )
        guard status == noErr, let raw = dataPointer else { return }

        let sampleCount = length / MemoryLayout<Float32>.size
        guard sampleCount > 0 else { return }

        // Rebind Int8* → Float32* and call Rust
        raw.withMemoryRebound(to: Float32.self, capacity: sampleCount) { floatPtr in
            callback(floatPtr, Int32(sampleCount), sampleRate)
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        NSLog("[AuraAudio] Stream stopped with error: %@", error.localizedDescription)
    }
}
