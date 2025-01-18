use cosmic::{
    app::{
        message::Message as CosmicMessage,
        Core, Settings, Task,
    },
    iced::{Alignment, Length, Pixels},
    widget, // Row, Column, slider, Container, etc.
    Application,
};

// ===============
// Bindings to ddcutil
mod bindings;
use bindings::{
    ddca_close_display,
    ddca_get_display_refs,
    ddca_init,
    ddca_open_display2,
    ddca_set_non_table_vcp_value,
    ddca_get_non_table_vcp_value, 
    DDCA_Display_Handle,
    DDCA_Display_Ref,

    // If your bindgen or a local struct includes something like:
    //   pub struct DDCA_Non_Table_Vcp_Value { pub current_value: u16, pub maximum_value: u16 }
    DDCA_Non_Table_Vcp_Value,
};

/// Your user-level messages.
#[derive(Clone, Debug)]
enum UserMessage {
    BrightnessChanged(usize, u8),
    BrightnessSetDone(usize, Result<(), String>),
}

type AppMsg = CosmicMessage<UserMessage>;

/// Info for each monitor: store a `DDCA_Display_Ref` and a `brightness` value read at startup.
#[derive(Clone)]
struct Monitor {
    display_ref: DDCA_Display_Ref,
    brightness: u8,
}

struct AppModel {
    core: Core,
    monitors: Vec<Monitor>,
}

// ===================
// Implementation
// ===================
impl Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = AppMsg;

    const APP_ID: &'static str = "com.iwakurasec.CosmicBrightness";

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        // 1) Initialize the library and enumerate the displays once.
        unsafe {
            let rc_init = ddca_init(std::ptr::null(), 2, 0);
            if rc_init != 0 {
                eprintln!("ddca_init failed with code {rc_init}");
            }
        }

        // 2) Grab references
        let mut monitors = Vec::new();
        unsafe {
            let mut drefs_ptr: *mut DDCA_Display_Ref = std::ptr::null_mut();
            let rc_enum = ddca_get_display_refs(false, &mut drefs_ptr);
            if rc_enum != 0 || drefs_ptr.is_null() {
                eprintln!("No displays found or ddca_get_display_refs failed (rc={rc_enum})");
            } else {
                let mut idx = 0;
                while !(*drefs_ptr.add(idx)).is_null() {
                    let dref = *drefs_ptr.add(idx);
                    idx += 1;

                    // 3) Open display, read brightness from VCP code 0x10, close
                    let brightness = read_brightness_vcp(dref);
                    monitors.push(Monitor { display_ref: dref, brightness });
                }
            }
        }

        let model = AppModel { core, monitors };
        (model, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            // Handle App messages
            CosmicMessage::App(user_msg) => match user_msg {
                UserMessage::BrightnessChanged(i, val) => {
                    // local update
                    if let Some(m) = self.monitors.get_mut(i) {
                        m.brightness = val;
                    }
                    // spawn async brightness update
                    Task::perform(
                        set_brightness_ddcutil(i, val, self.monitors.clone()),
                        move |res| {
                            // Wrap the result back into AppMsg
                            CosmicMessage::App(CosmicMessage::App(UserMessage::BrightnessSetDone(i, res))) //this nested structure feels really wrong but this is the only way to get it to work without errors.
                        },
                    )
                }
                UserMessage::BrightnessSetDone(i, Ok(())) => {
                    println!("Monitor {i} brightness set OK!");
                    Task::none()
                }
                UserMessage::BrightnessSetDone(i, Err(e)) => {
                    eprintln!("Monitor {i} error: {e}");
                    Task::none()
                }
            },
            // Handle Cosmic events
            CosmicMessage::Cosmic(evt) => {
                eprintln!("cosmic event: {evt:?}");
                Task::none()
            }
            // Handle None messages
            CosmicMessage::None => Task::none(),
            // Handle DbusActivation if feature is enabled
            #[cfg(feature = "single-instance")]
            Message::DbusActivation(_) => Task::none(),
        }
    }

    fn view(&self) -> cosmic::Element<Self::Message> {
        let mut row_of_sliders = widget::Row::new().spacing(20).align_y(Alignment::Center);

        for (i, mon) in self.monitors.iter().enumerate() {
            // Single wrap
            let slider = widget::slider(
                0..=100,
                mon.brightness,
                move |val| {
                    CosmicMessage::App(UserMessage::BrightnessChanged(i, val))
                },
            )
            .step(1)
            .height(Pixels::from(200));

            let label = widget::text::body(format!("Monitor {i} (Current={})", mon.brightness));

            let col = widget::Column::new()
                .push(slider)
                .push(label)
                .align_x(Alignment::Center)
                .spacing(10);

            row_of_sliders = row_of_sliders.push(col);
        }

        widget::Container::new(row_of_sliders)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Shrink)
            .center_y(Length::Shrink)
            .into()
    }
}

fn main() -> cosmic::iced::Result {
    let settings = Settings::default();
    cosmic::app::run::<AppModel>(settings, ())
}

// ======================
// Helpers
// ======================
use tokio::task::spawn_blocking;

impl DDCA_Non_Table_Vcp_Value {
    pub fn current_value(&self) -> u16 {
        ((self.sh as u16) << 8) | (self.sl as u16)
    }
    pub fn maximum_value(&self) -> u16 {
        ((self.mh as u16) << 8) | (self.ml as u16)
    }
}

/// A small helper that opens the display, reads brightness from VCP code 0x10, closes.
unsafe fn read_brightness_vcp(dref: DDCA_Display_Ref) -> u8 {
    let mut brightness = 50;
    let mut handle: DDCA_Display_Handle = std::ptr::null_mut();
    if ddca_open_display2(dref, false, &mut handle) == 0 && !handle.is_null() {
        let mut val: DDCA_Non_Table_Vcp_Value = std::mem::zeroed();
        let rc_get = ddca_get_non_table_vcp_value(handle, 0x10, &mut val);
        if rc_get == 0 {
            brightness = val.current_value().min(1000) as u8; // clamp if you prefer
        } else {
            eprintln!("ddca_get_non_table_vcp_value failed rc={rc_get}");
        }
        ddca_close_display(handle);
    }
    brightness
}
unsafe impl Send for Monitor {}

/// Async brightness setter.  
async fn set_brightness_ddcutil(
    index: usize,
    value: u8,
    monitors: Vec<Monitor>,
) -> Result<(), String> {
    spawn_blocking(move || unsafe {
        let Some(m) = monitors.get(index) else {
            return Err(format!("No such monitor: {index}"));
        };

        let mut handle: DDCA_Display_Handle = std::ptr::null_mut();
        let rc_open = ddca_open_display2(m.display_ref, false, &mut handle);
        if rc_open != 0 {
            return Err(format!("ddca_open_display2 rc={rc_open}"));
        }

        let rc_set = ddca_set_non_table_vcp_value(handle, 0x10, 0, value);
        if rc_set != 0 {
            ddca_close_display(handle);
            return Err(format!("ddca_set_non_table_vcp_value rc={rc_set}"));
        }

        let rc_close = ddca_close_display(handle);
        if rc_close != 0 {
            eprintln!("Warning: ddca_close_display rc={rc_close}");
        }
        Ok(())
    })
    .await
    .map_err(|join_err| format!("Tokio join error: {join_err}"))?
}
