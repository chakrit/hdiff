#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Key,
    Resize { width: u16, height: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStage {
    RawMode,
    AlternateScreen,
    HiddenCursor,
}

pub fn coalesce_resize(events: Vec<Event>) -> Vec<Event> {
    let mut coalesced = Vec::new();
    let mut pending_resize = None;

    for event in events {
        match event {
            Event::Resize { .. } => pending_resize = Some(event),
            event => {
                if let Some(resize) = pending_resize.take() {
                    coalesced.push(resize);
                }
                coalesced.push(event);
            }
        }
    }

    if let Some(resize) = pending_resize {
        coalesced.push(resize);
    }

    coalesced
}

pub fn cleanup_order(stages: &[SetupStage]) -> Vec<SetupStage> {
    stages.iter().rev().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::{Event, SetupStage, cleanup_order, coalesce_resize};

    #[test]
    fn coalesces_a_resize_burst_to_its_latest_dimensions() {
        let events = vec![
            Event::Resize {
                width: 80,
                height: 20,
            },
            Event::Resize {
                width: 100,
                height: 30,
            },
            Event::Key,
        ];

        assert_eq!(
            coalesce_resize(events),
            vec![
                Event::Resize {
                    width: 100,
                    height: 30,
                },
                Event::Key,
            ]
        );
    }

    #[test]
    fn unwinds_completed_setup_in_reverse_order() {
        let completed = [
            SetupStage::RawMode,
            SetupStage::AlternateScreen,
            SetupStage::HiddenCursor,
        ];

        assert_eq!(
            cleanup_order(&completed),
            vec![
                SetupStage::HiddenCursor,
                SetupStage::AlternateScreen,
                SetupStage::RawMode,
            ]
        );
    }
}
