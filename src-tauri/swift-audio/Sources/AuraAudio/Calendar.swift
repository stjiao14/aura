import Foundation
import EventKit

// MARK: - Public FFI entry points

/// Returns a JSON string with upcoming calendar events (next 24 hours).
/// The caller must free the returned pointer via `aura_calendar_events_free`.
/// Returns nil if calendar access is denied.
@_silgen_name("aura_calendar_events")
public func auraCalendarEvents() -> UnsafePointer<CChar>? {
    let store = EKEventStore()
    var granted = false

    let semaphore = DispatchSemaphore(value: 0)

    if #available(macOS 14.0, *) {
        store.requestFullAccessToEvents { ok, error in
            granted = ok
            if let error = error {
                NSLog("[AuraCalendar] requestFullAccessToEvents error: %@", error.localizedDescription)
            }
            semaphore.signal()
        }
    } else {
        store.requestAccess(to: .event) { ok, error in
            granted = ok
            if let error = error {
                NSLog("[AuraCalendar] requestAccess error: %@", error.localizedDescription)
            }
            semaphore.signal()
        }
    }

    semaphore.wait()

    guard granted else {
        NSLog("[AuraCalendar] Calendar access denied")
        return nil
    }

    let now = Date()
    let tomorrow = now.addingTimeInterval(86400)
    let predicate = store.predicateForEvents(withStart: now, end: tomorrow, calendars: nil)
    let events = store.events(matching: predicate)

    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]

    // Meeting URL patterns
    let urlPatterns = [
        "https://[\\w.-]*zoom\\.us/j/\\S+",
        "https://teams\\.microsoft\\.com/l/meetup-join/\\S+",
        "https://meet\\.google\\.com/\\S+",
    ]

    var result: [[String: Any]] = []
    for event in events {
        // Try event.url first, then scan notes for meeting URLs
        var meetingUrl: String? = event.url?.absoluteString
        if meetingUrl == nil, let notes = event.notes {
            for pattern in urlPatterns {
                if let range = notes.range(of: pattern, options: .regularExpression) {
                    meetingUrl = String(notes[range])
                    break
                }
            }
        }

        let attendees = event.attendees?.compactMap { participant -> String? in
            participant.name ?? participant.url.absoluteString
        } ?? []

        let dict: [String: Any] = [
            "title": event.title ?? "Untitled",
            "startDate": formatter.string(from: event.startDate),
            "endDate": formatter.string(from: event.endDate),
            "meetingUrl": meetingUrl as Any,
            "attendees": attendees,
            "calendarName": event.calendar.title,
        ]
        result.append(dict)
    }

    guard let jsonData = try? JSONSerialization.data(withJSONObject: result),
          let jsonStr = String(data: jsonData, encoding: .utf8) else {
        NSLog("[AuraCalendar] Failed to serialize events to JSON")
        return nil
    }

    NSLog("[AuraCalendar] Returning %d events", result.count)
    return strdup(jsonStr).map { UnsafePointer($0) }
}

/// Free a JSON string returned by `aura_calendar_events`.
@_silgen_name("aura_calendar_events_free")
public func auraCalendarEventsFree(ptr: UnsafeMutablePointer<CChar>?) {
    free(ptr)
}
