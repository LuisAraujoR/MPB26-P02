use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

use esp_idf_svc::hal::gpio::{InterruptType, Output, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::sys::EspError;

// Banderas atómicas para comunicar las interrupciones con el bucle principal de forma segura
static BOTON_I_PRESIONADO: AtomicBool = AtomicBool::new(false);
static BOTON_R_PRESIONADO: AtomicBool = AtomicBool::new(false);

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

    log::info!("Iniciando control de 74HC595 con interrupciones...");

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

    let mut sr = ShiftRegister595::new(data_pin, clr_pin, load_pin, clk_pin)?;

    // Entradas con resistencia Pull-Up interna
    let mut boton_i = PinDriver::input(peripherals.pins.gpio46, Pull::Up)?;
    let mut boton_r = PinDriver::input(peripherals.pins.gpio3, Pull::Up)?;

    // Configurar tipo de interrupción: NegEdge (flanco de bajada al presionar hacia GND)
    boton_i.set_interrupt_type(InterruptType::NegEdge)?;
    boton_r.set_interrupt_type(InterruptType::NegEdge)?;

    // Suscribir manejadores de interrupción (ISR).
    // Las ISR deben ser mínimas: únicamente activan la bandera atómica.
    unsafe {
        boton_i.subscribe(|| {
            BOTON_I_PRESIONADO.store(true, Ordering::Relaxed);
        })?;

        boton_r.subscribe(|| {
            BOTON_R_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
    }

    // Habilitar interrupciones
    boton_i.enable_interrupt()?;
    boton_r.enable_interrupt()?;

    log::info!("Interrupciones configuradas en boton_i (GPIO46) y boton_r (GPIO3)");

    let mut min: u8 = 0;
    let mut seg: u8 = 0;
    let mut ec: bool = false; // Estado del contador: false = detenido, true = en marcha

    // Mostrar 00:00 al inicio
    sr.send_byte(decto7seg(0))?;
    sr.send_byte(decto7seg(0))?;
    sr.send_byte(decto7seg(0))?;

    loop {
        // --- 1. Gestionar interrupción del botón Inicio / Parada (boton_i) ---
        if BOTON_I_PRESIONADO.swap(false, Ordering::Relaxed) {
            ec = !ec;
            log::info!(
                "Botón I pulsado: Contador {}",
                if ec { "INICIADO" } else { "DETENIDO" }
            );

            // Tiempo antirrebote y espera para rearmar la interrupción
            sleep(Duration::from_millis(150));
            // En esp-idf-hal la interrupción se deshabilita automáticamente al dispararse;
            // se debe volver a habilitar desde el contexto principal:
            boton_i.enable_interrupt()?;
        }

        // --- 2. Gestionar interrupción del botón Reset (boton_r) ---
        if BOTON_R_PRESIONADO.swap(false, Ordering::Relaxed) {
            if !ec {
                // Solo se reinicia si el contador está detenido
                min = 0;
                seg = 0;
                sr.send_byte(decto7seg(0))?;
                sr.send_byte(decto7seg(0))?;
                sr.send_byte(decto7seg(0))?;
                log::info!("Botón R pulsado: Contador reiniciado a 00:00");
            } else {
                log::info!("Botón R pulsado ignorado (el contador está en marcha)");
            }

            // Tiempo antirrebote y rearmar la interrupción
            sleep(Duration::from_millis(150));
            boton_r.enable_interrupt()?;
        }

        // --- 3. Lógica de avance del contador si está en marcha ---
        if ec {
            // Dormimos 1 segundo (1000 ms) en intervalos de 50 ms.
            // Si durante este lapso se pulsa el botón de parada, la interrupción se captura
            // de inmediato y salimos del ciclo sin esperar a que termine el segundo.
            for _ in 0..20 {
                if BOTON_I_PRESIONADO.load(Ordering::Relaxed) {
                    break;
                }
                sleep(Duration::from_millis(50));
            }

            // Si al finalizar la espera sigue en marcha y no se solicitó parar, incrementamos el tiempo
            if ec && !BOTON_I_PRESIONADO.load(Ordering::Relaxed) {
                seg += 1;
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

                log::info!("Tiempo: {:02}:{:02}", min, seg);
            }
        } else {
            // Si el contador está detenido, dormimos un breve periodo para ceder la CPU
            sleep(Duration::from_millis(20));
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
