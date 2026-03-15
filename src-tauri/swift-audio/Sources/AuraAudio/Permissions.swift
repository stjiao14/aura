import AVFoundation
import CoreGraphics
import EventKit

// MARK: - Microphone

/// Check current microphone authorization status.
/// Returns: 0 = not determined, 1 = authorized, 2 = denied, 3 = restricted
@_silgen_name("aura_mic_auth_status")
public func auraMicAuthStatus() -> Int32 {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .notDetermined: return 0
    case .authorized:    return 1
    case .denied:        return 2
    case .restricted:    return 3
    @unknown default:    return 0
    }
}

/// Request microphone access. Blocks until the user responds.
/// Returns true if access was granted.
@_silgen_name("aura_request_mic_access")
public func auraRequestMicAccess() -> Bool {
    let semaphore = DispatchSemaphore(value: 0)
    var granted = false
    AVCaptureDevice.requestAccess(for: .audio) { ok in
        granted = ok
        semaphore.signal()
    }
    semaphore.wait()
    return granted
}

// MARK: - Screen Recording

/// Check if screen recording permission is already granted (macOS 13+).
/// Returns true if authorized, false if not yet determined or denied.
@_silgen_name("aura_screen_recording_check")
public func auraScreenRecordingCheck() -> Bool {
    if #available(macOS 13.0, *) {
        return CGPreflightScreenCaptureAccess()
    }
    return true // assume granted on older macOS
}

/// Request screen recording permission (macOS 13+).
/// On macOS 13+: opens System Settings if not already authorized.
/// Returns true if authorized.
@_silgen_name("aura_screen_recording_request")
public func auraScreenRecordingRequest() -> Bool {
    if #available(macOS 13.0, *) {
        return CGRequestScreenCaptureAccess()
    }
    return true
}

// MARK: - Calendar

/// Check current calendar authorization status.
/// Returns: 0 = not determined, 1 = authorized, 2 = denied/restricted
@_silgen_name("aura_calendar_auth_status")
public func auraCalendarAuthStatus() -> Int32 {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(macOS 14.0, *) {
        switch status {
        case .notDetermined:     return 0
        case .fullAccess:        return 1
        case .writeOnly:         return 1
        case .authorized:        return 1
        case .denied, .restricted: return 2
        @unknown default:        return 0
        }
    } else {
        switch status {
        case .notDetermined: return 0
        case .authorized:    return 1
        case .denied:        return 2
        case .restricted:    return 2
        @unknown default:    return 0
        }
    }
}

/// Request calendar access. Blocks until the user responds.
/// Returns true if access was granted.
@_silgen_name("aura_request_calendar_access")
public func auraRequestCalendarAccess() -> Bool {
    let store = EKEventStore()
    let semaphore = DispatchSemaphore(value: 0)
    var granted = false

    if #available(macOS 14.0, *) {
        store.requestFullAccessToEvents { ok, _ in
            granted = ok
            semaphore.signal()
        }
    } else {
        store.requestAccess(to: .event) { ok, _ in
            granted = ok
            semaphore.signal()
        }
    }

    semaphore.wait()
    return granted
}
