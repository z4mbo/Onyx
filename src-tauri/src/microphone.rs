#[cfg(target_os = "macos")]
mod macos {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    use std::sync::{Arc, Mutex};
    use tokio::{
        sync::oneshot,
        time::{Duration, timeout},
    };

    fn audio_media_type() -> Result<&'static objc2_av_foundation::AVMediaType, String> {
        // SAFETY: AVMediaTypeAudio is an immutable AVFoundation framework constant.
        unsafe { AVMediaTypeAudio }
            .ok_or_else(|| "macOS did not provide the audio permission service.".to_string())
    }

    fn status_name(status: AVAuthorizationStatus) -> &'static str {
        match status {
            AVAuthorizationStatus::Authorized => "authorized",
            AVAuthorizationStatus::Denied => "denied",
            AVAuthorizationStatus::Restricted => "restricted",
            _ => "not_determined",
        }
    }

    fn begin_access_request(
        media_type: &'static objc2_av_foundation::AVMediaType,
    ) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let completion = RcBlock::new({
            let sender = Arc::clone(&sender);
            move |granted: Bool| {
                if let Ok(mut sender) = sender.lock()
                    && let Some(sender) = sender.take()
                {
                    let _ = sender.send(granted.as_bool());
                }
            }
        });

        // SAFETY: media_type is the AVFoundation-provided audio constant. The
        // escaping block owns its sender and AVFoundation copies the block.
        unsafe {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &completion);
        }
        receiver
    }

    pub async fn request_access() -> Result<String, String> {
        let media_type = audio_media_type()?;
        // SAFETY: media_type is the AVFoundation-provided AVMediaTypeAudio constant.
        let current = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
        if current == AVAuthorizationStatus::Authorized {
            return Ok(status_name(current).to_string());
        }
        if current == AVAuthorizationStatus::Denied {
            return Err(
                "Microphone access is off. Enable Onyx in System Settings → Privacy & Security → Microphone, then relaunch Onyx."
                    .to_string(),
            );
        }
        if current == AVAuthorizationStatus::Restricted {
            return Err(
                "Microphone access is restricted by macOS or device management.".to_string(),
            );
        }

        // Keep non-Send Objective-C block handles out of the async state machine.
        let receiver = begin_access_request(media_type);

        match timeout(Duration::from_secs(90), receiver).await {
            Ok(Ok(true)) => Ok("authorized".to_string()),
            Ok(Ok(false)) => Err(
                "Microphone access was denied. Enable Onyx in System Settings → Privacy & Security → Microphone."
                    .to_string(),
            ),
            Ok(Err(_)) => Err("macOS closed the microphone permission request.".to_string()),
            Err(_) => Err("The macOS microphone permission request timed out.".to_string()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn permission_statuses_are_stable_for_the_webview() {
            assert_eq!(status_name(AVAuthorizationStatus::Authorized), "authorized");
            assert_eq!(status_name(AVAuthorizationStatus::Denied), "denied");
            assert_eq!(status_name(AVAuthorizationStatus::Restricted), "restricted");
            assert_eq!(
                status_name(AVAuthorizationStatus::NotDetermined),
                "not_determined"
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::request_access;

#[cfg(not(target_os = "macos"))]
pub async fn request_access() -> Result<String, String> {
    Ok("webview".to_string())
}
