use std::thread::sleep;
use std::time::Duration;

use esp_idf_svc::hal::gpio::{Output, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::sys::EspError;

pub struct ShiftRegister595<'d> {
    data: PinDriver<'d, Output>,
    clr: PinDriver<'d, Output>,
    load: PinDriver<'d, Output>,
    clk: PinDriver<'d, Output>,
}

impl<'d> ShiftRegister595<'d> {
    /// Inicializa los pines del 74HC595:
    /// - `data`: Pin SER / DS (datos seriales)
    /// - `clr`: Pin SRCLR / MR (limpieza asíncrona, activo en bajo)
    /// - `load`: Pin RCLK / STCP (latch / reloj de almacenamiento)
    /// - `clk`: Pin SRCLK / SHCP (reloj de desplazamiento)
    pub fn new(
        mut data: PinDriver<'d, Output>,
        mut clr: PinDriver<'d, Output>,
        mut load: PinDriver<'d, Output>,
        mut clk: PinDriver<'d, Output>,
    ) -> Result<Self, EspError> {
        // Estado inicial de reposo
        data.set_low()?;
        clk.set_low()?;
        load.set_low()?;
        // CLR es activo en bajo; debe estar en ALTO para permitir el funcionamiento normal
        clr.set_high()?;

        let mut sr = Self {
            data,
            clr,
            load,
            clk,
        };
        // Limpiamos el registro al iniciar
        sr.clear()?;
        Ok(sr)
    }

    /// Envía un byte (8 bits) al 74HC595.
    /// Envía el bit más significativo (MSB, bit 7) primero, de modo que:
    /// - bit 0 -> Salida Q0
    /// - bit 1 -> Salida Q1
    /// ...
    /// - bit 7 -> Salida Q7
    pub fn send_byte(&mut self, byte: u8) -> Result<(), EspError> {
        self.load.set_low()?;
        self.clk.set_low()?;

        // Aseguramos que CLR esté en alto
        self.clr.set_high()?;

        // Enviar 8 bits (del MSB bit 7 al LSB bit 0)
        for i in (0..8).rev() {
            let bit = (byte >> i) & 1;
            if bit == 1 {
                self.data.set_high()?;
            } else {
                self.data.set_low()?;
            }

            // Flanco de subida en CLK para ingresar el bit al registro de desplazamiento
            self.clk.set_high()?;
            self.clk.set_low()?;
        }

        // Flanco de subida en LOAD (RCLK) para pasar los datos a las salidas (latch)
        self.load.set_high()?;
        self.load.set_low()?;

        Ok(())
    }

    /// Limpia el registro de desplazamiento usando el pin CLR (activo en bajo)
    /// y actualiza las salidas a 0.
    pub fn clear(&mut self) -> Result<(), EspError> {
        // Pulso bajo en CLR para borrar los registros internos
        self.clr.set_low()?;
        self.clr.set_high()?;

        // Actualizar las salidas
        self.load.set_high()?;
        self.load.set_low()?;

        Ok(())
    }
}

fn main() -> Result<(), EspError> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Iniciando control de 74HC595...");

    let peripherals = Peripherals::take()?;

    // Mapeo de pines según la conexión del usuario:
    // DATA -> GPIO 38
    // CLR  -> GPIO 39
    // LOAD -> GPIO 40
    // CLK  -> GPIO 41
    let data_pin = PinDriver::output(peripherals.pins.gpio38)?;
    let clr_pin = PinDriver::output(peripherals.pins.gpio39)?;
    let load_pin = PinDriver::output(peripherals.pins.gpio40)?;
    let clk_pin = PinDriver::output(peripherals.pins.gpio41)?;

    // Entradas
    let boton_i = PinDriver::input(peripherals.pins.gpio46, Pull::Up)?;
    let boton_r = PinDriver::input(peripherals.pins.gpio3, Pull::Up)?;

    let mut sr = ShiftRegister595::new(data_pin, clr_pin, load_pin, clk_pin)?;

    log::info!("Secuencia: Encendiendo un LED a la vez (Q0 a Q7)...");

    let mut min: u8 = 0;
    let mut seg: u8 = 0;

    let mut ec: bool = false;

    sr.send_byte(decto7seg(0))?;
    sr.send_byte(decto7seg(0))?;
    sr.send_byte(decto7seg(0))?;

    loop {
        log::info!(
            "BotonI {}     BotonR {}",
            boton_i.is_low(),
            boton_r.is_low()
        );
        //1//for x in 0..=200 {
        //1//    let c: u8 = x / 100;
        //1//    let d: u8 = (x - c * 100) / 10;
        //1//    let u: u8 = x % 10;
        //1//
        //1//    sr.send_byte(decto7seg(u))?;
        //1//    sr.send_byte(decto7seg(d))?;
        //1//    sr.send_byte(decto7seg(c))?;
        //1//
        //1//    sleep(Duration::from_millis(100));
        //1//}

        //2//let ds: u8 = seg / 10;
        //2//let us: u8 = seg % 10;
        //2//
        //2//sr.send_byte(decto7seg(us))?;
        //2//sr.send_byte(decto7seg(ds))?;
        //2//sr.send_byte(decto7seg(min))?;
        //2//
        //2//sleep(Duration::from_millis(100));
        //2//
        //2//seg = seg + 1;
        //2//if seg == 60 {
        //2//    seg = 0;
        //2//    min += 1;
        //2//    if min == 10 {
        //2//        min = 0;
        //2//    }
        //2//}

        if ec == false && boton_i.is_low() {
            ec = true;

            while boton_i.is_low() {
                sleep(Duration::from_millis(100));
            }
        }

        if ec == true && boton_i.is_low() {
            ec = false;
            while boton_i.is_low() {
                sleep(Duration::from_millis(100));
            }
        }

        sleep(Duration::from_millis(100));

        if ec == false && boton_r.is_low() {
            sr.send_byte(decto7seg(0))?;
            sr.send_byte(decto7seg(0))?;
            sr.send_byte(decto7seg(0))?;

            min = 0;
            seg = 0;
        }

        if ec == true {
            seg = seg + 1;
            if seg == 60 {
                seg = 0;
                min += 1;
                if min == 10 {
                    min = 0;
                }
            }

            let ds: u8 = seg / 10;
            let us: u8 = seg % 10;

            sr.send_byte(decto7seg(us))?;
            sr.send_byte(decto7seg(ds))?;
            sr.send_byte(decto7seg(min))?;

            sleep(Duration::from_millis(900));
        }
    }
}

fn decto7seg(numero: u8) -> u8 {
    match numero {
        0 => 0b00111111,
        1 => 0b00000110,
        2 => 0b01011011,
        3 => 0b01001111,
        4 => 0b01100110,
        5 => 0b01101101,
        6 => 0b01111101,
        7 => 0b00000111,
        8 => 0b01111111,
        9 => 0b01100111,
        _ => 0b00000000,
    }
}
