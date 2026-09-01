import Foundation
import AppKit
import ScreenCaptureKit
import Vision

struct OcrObservation: Encodable {
    let text: String
    let confidence: Float
    let x: CGFloat
    let y: CGFloat
    let width: CGFloat
    let height: CGFloat
}

struct OcrOutput: Encodable {
    let pid: Int32
    let windowID: UInt32
    let windowX: CGFloat
    let windowY: CGFloat
    let windowWidth: CGFloat
    let windowHeight: CGFloat
    let width: Int
    let height: Int
    let observations: [OcrObservation]
    let warnings: [String]
}

@main
struct WindowOcr {
    @MainActor
    static func main() async {
        guard let pid = argumentInt("--pid") else {
            fail("missing --pid")
        }

        do {
            // ScreenCaptureKit queries WindowServer through CoreGraphics. A CLI
            // process must establish the AppKit/CoreGraphics connection first.
            _ = CGMainDisplayID()
            let application = NSApplication.shared
            application.setActivationPolicy(.accessory)
            application.finishLaunching()

            let content = try await SCShareableContent.excludingDesktopWindows(
                false,
                onScreenWindowsOnly: true
            )
            guard let window = content.windows.first(where: {
                $0.owningApplication?.processID == pid && $0.windowLayer == 0
            }) else {
                fail("target application window was not found")
            }

            let filter = SCContentFilter(desktopIndependentWindow: window)
            let configuration = SCStreamConfiguration()
            configuration.width = max(Int(window.frame.width * 2), 1)
            configuration.height = max(Int(window.frame.height * 2), 1)
            configuration.scalesToFit = false
            configuration.showsCursor = false

            let image = try await SCScreenshotManager.captureImage(
                contentFilter: filter,
                configuration: configuration
            )
            let request = VNRecognizeTextRequest()
            request.recognitionLevel = .accurate
            request.usesLanguageCorrection = false
            request.recognitionLanguages = ["zh-Hans", "en-US"]
            request.minimumTextHeight = 0.005

            let handler = VNImageRequestHandler(cgImage: image, options: [:])
            try handler.perform([request])
            let observations = (request.results ?? []).compactMap { observation -> OcrObservation? in
                guard let candidate = observation.topCandidates(1).first else {
                    return nil
                }
                let box = observation.boundingBox
                return OcrObservation(
                    text: candidate.string,
                    confidence: candidate.confidence,
                    x: box.origin.x,
                    y: box.origin.y,
                    width: box.size.width,
                    height: box.size.height
                )
            }
            emit(OcrOutput(
                pid: pid,
                windowID: window.windowID,
                windowX: window.frame.origin.x,
                windowY: window.frame.origin.y,
                windowWidth: window.frame.width,
                windowHeight: window.frame.height,
                width: image.width,
                height: image.height,
                observations: observations,
                warnings: []
            ))
        } catch {
            fail("ScreenCaptureKit or Vision failed: \(error)")
        }
    }

    private static func argumentInt(_ name: String) -> Int32? {
        guard let index = CommandLine.arguments.firstIndex(of: name),
              index + 1 < CommandLine.arguments.count else {
            return nil
        }
        return Int32(CommandLine.arguments[index + 1])
    }

    private static func emit(_ output: OcrOutput) {
        do {
            let data = try JSONEncoder().encode(output)
            FileHandle.standardOutput.write(data)
            FileHandle.standardOutput.write(Data([10]))
        } catch {
            fail("could not encode OCR result: \(error)")
        }
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data((message + "\n").utf8))
        exit(1)
    }
}
