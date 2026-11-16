//! Embedded gamepad button legends, decoded once without runtime file paths.
use gtk::cairo::ImageSurface;

pub struct Assets {
    pub buttons: Option<[ImageSurface; 4]>,
}
impl Assets {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(feature = "private-assets")]
        {
            use std::io::Cursor;
            let decode = |bytes: &[u8]| ImageSurface::create_from_png(&mut Cursor::new(bytes));
            Ok(Self {
                buttons: Some([
                    decode(include_bytes!("../assets/Legend_A.png"))?,
                    decode(include_bytes!("../assets/Legend_B.png"))?,
                    decode(include_bytes!("../assets/Legend_X.png"))?,
                    decode(include_bytes!("../assets/Legend_Y.png"))?,
                ]),
            })
        }
        #[cfg(not(feature = "private-assets"))]
        Ok(Self { buttons: None })
    }
}

thread_local! {
    pub(crate) static LOCAL: Assets = Assets::load().unwrap_or_else(|error| {
        eprintln!("Could not decode embedded button PNGs: {error}; using text labels");
        Assets { buttons: None }
    });
}

#[cfg(all(test, feature = "private-assets"))]
mod tests {
    #[test]
    fn supplied_buttons_decode_with_transparency() {
        for mut surface in super::Assets::load().unwrap().buttons.unwrap() {
            assert_eq!((surface.width(), surface.height()), (32, 32));
            assert_eq!(surface.format(), gtk::cairo::Format::ARgb32);
            let bytes = surface.data().unwrap();
            let alpha = if cfg!(target_endian = "little") { 3 } else { 0 };
            assert!(bytes.as_chunks::<4>().0.iter().any(|p| p[alpha] == 0));
            assert!(bytes.as_chunks::<4>().0.iter().any(|p| p[alpha] > 240));
        }
    }
}
