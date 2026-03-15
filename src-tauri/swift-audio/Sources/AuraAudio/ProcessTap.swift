import AVFAudio
import CoreAudio

/// System audio capture via AudioHardwareCreateProcessTap (macOS 14.2+).
/// Shows the yellow mic indicator instead of the purple screen-sharing one.
@available(macOS 14.2, *)
final class ProcessTapLoopback {
    private var tapID: AudioObjectID = 0
    private var aggregateID: AudioObjectID = 0
    private var engine: AVAudioEngine?
    private let callback: AudioChunkCallback

    init(callback: @escaping AudioChunkCallback) {
        self.callback = callback
    }

    func start() -> Bool {
        // 1. Create a global process tap that captures all system audio
        //    except our own process (to avoid feedback loops).
        let myPID = AudioObjectID(ProcessInfo.processInfo.processIdentifier)
        let tapDesc = CATapDescription(stereoGlobalTapButExcludeProcesses: [myPID])
        tapDesc.name = "Aura System Audio"

        var tapObjectID: AudioObjectID = 0
        var status = AudioHardwareCreateProcessTap(tapDesc, &tapObjectID)
        guard status == noErr else {
            NSLog("[AuraAudio] ProcessTap creation failed: %d", status)
            return false
        }
        self.tapID = tapObjectID

        // 2. Get the tap's device UID
        guard let tapUID = deviceUID(for: tapObjectID) else {
            NSLog("[AuraAudio] Failed to get tap device UID")
            cleanup()
            return false
        }

        // 3. Get the default output device UID — needed as the clock source
        //    for the aggregate device.
        guard let outputUID = defaultOutputDeviceUID() else {
            NSLog("[AuraAudio] Failed to get default output device UID")
            cleanup()
            return false
        }

        // 4. Create a private aggregate device that pairs the output device
        //    (for clocking) with our process tap (for audio data).
        let aggDesc: NSDictionary = [
            kAudioAggregateDeviceNameKey: "Aura Tap",
            kAudioAggregateDeviceUIDKey: "com.aura.process-tap-\(UUID().uuidString)",
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [
                [kAudioSubDeviceUIDKey: outputUID]
            ],
            kAudioAggregateDeviceTapListKey: [
                [kAudioSubTapUIDKey: tapUID]
            ],
        ]

        var aggObjectID: AudioObjectID = 0
        status = AudioHardwareCreateAggregateDevice(aggDesc, &aggObjectID)
        guard status == noErr else {
            NSLog("[AuraAudio] Aggregate device creation failed: %d", status)
            cleanup()
            return false
        }
        self.aggregateID = aggObjectID

        // 5. Set up AVAudioEngine to capture from the aggregate device.
        let engine = AVAudioEngine()
        let inputNode = engine.inputNode

        var deviceID = aggObjectID
        status = AudioUnitSetProperty(
            inputNode.audioUnit!,
            kAudioOutputUnitProperty_CurrentDevice,
            kAudioUnitScope_Global,
            0,
            &deviceID,
            UInt32(MemoryLayout<AudioObjectID>.size)
        )
        guard status == noErr else {
            NSLog("[AuraAudio] Failed to set aggregate as input device: %d", status)
            cleanup()
            return false
        }

        let format = inputNode.outputFormat(forBus: 0)
        let cb = self.callback

        inputNode.installTap(onBus: 0, bufferSize: 4096, format: format) { buffer, _ in
            guard let channelData = buffer.floatChannelData else { return }
            let frameCount = Int(buffer.frameLength)
            let sampleRate = format.sampleRate

            if format.channelCount >= 2 {
                // Mix stereo → mono
                var mono = [Float](repeating: 0, count: frameCount)
                for i in 0..<frameCount {
                    mono[i] = (channelData[0][i] + channelData[1][i]) * 0.5
                }
                mono.withUnsafeBufferPointer { ptr in
                    cb(ptr.baseAddress, Int32(frameCount), sampleRate)
                }
            } else {
                cb(channelData[0], Int32(frameCount), sampleRate)
            }
        }

        do {
            try engine.start()
            self.engine = engine
            NSLog("[AuraAudio] Process tap capture started (%.0f Hz, %d ch)",
                  format.sampleRate, format.channelCount)
            return true
        } catch {
            NSLog("[AuraAudio] AVAudioEngine start failed: %@", error.localizedDescription)
            cleanup()
            return false
        }
    }

    func stop() {
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        engine = nil
        cleanup()
        NSLog("[AuraAudio] Process tap capture stopped")
    }

    private func cleanup() {
        if aggregateID != 0 {
            AudioHardwareDestroyAggregateDevice(aggregateID)
            aggregateID = 0
        }
        if tapID != 0 {
            AudioHardwareDestroyProcessTap(tapID)
            tapID = 0
        }
    }

    // MARK: - Helpers

    private func deviceUID(for deviceID: AudioObjectID) -> String? {
        var uid: Unmanaged<CFString>?
        var propSize = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        var addr = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyDeviceUID,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let status = AudioObjectGetPropertyData(deviceID, &addr, 0, nil, &propSize, &uid)
        guard status == noErr, let cf = uid else { return nil }
        return cf.takeUnretainedValue() as String
    }

    private func defaultOutputDeviceUID() -> String? {
        var outputID: AudioObjectID = 0
        var propSize = UInt32(MemoryLayout<AudioObjectID>.size)
        var addr = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &propSize, &outputID
        )
        guard status == noErr else { return nil }
        return deviceUID(for: outputID)
    }
}
