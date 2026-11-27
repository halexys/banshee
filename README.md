# Banshee

Panel de control para el **MSI Prestige 16 AI Studio B1VFG** (EC `15A2EMS1`) en Linux, **basado en
[GhostDeck](https://github.com/wygodad/ghostdeck)** de Dawid Wygoda (Windows). Un solo ejecutable
(Tauri 2 + Rust).

> Banshee es un proyecto independiente: no es GhostDeck ni una versión oficial suya, y su autor no lo
> respalda ni le da soporte.

## Qué hace

- **4 perfiles**: Silencioso, Equilibrado, Extremo y Super Batería (+ Fan Boost y reset de pánico).
- **Curva de ventiladores** CPU y GPU, de 0 a 150 % (150 = velocidad máxima real), con formas rápidas,
  zonas de ruido, punto en vivo y asignación por perfil.
- **Límite de carga** (60 / 80 / 100) y modo viaje (100 % durante 7 días).
- **Teclado** (retroiluminación 0-3), **Fn ⇄ Win** y **modo gráfico** Híbrido / Solo integrada (al reiniciar).
- **Estado en vivo**: temperaturas, rpm, vatios de CPU y GPU (sin despertar la RTX), batería.
- **Barra de GNOME**: extensión propia (va dentro del binario) con el banshee del color del perfil,
  temperaturas con aviso de color y notificación si CPU/GPU pasan de 90 °C durante 10 s. Un clic abre el
  panel rápido. Sin GNOME, icono de bandeja normal.
- Restaura perfil, curva, límite y teclado al arrancar y al volver de suspensión.

## Requisitos

- Módulo [msi-ec](https://github.com/MControlSuite/msi-ec) con soporte para `15A2EMS1.114` (PR #803 más una
  configuración completa `CONF_G2_11`), instalado por DKMS y firmado con MOK si hay Secure Boot.
- `pkexec` (polkit) para la configuración inicial y el cambio de modo gráfico.
- Opcional: `nvidia-smi` para la telemetría de la RTX.

## Compilar e instalar

```sh
cargo build --release
./install.sh            # ~/.local/bin/banshee + menú de aplicaciones + arranque automático
```

La primera vez, en la app, **Activar control**: instala (como root, vía pkexec) la regla udev que da al
usuario permiso sobre los atributos de msi-ec y la opción `debug=1` del módulo.

## Línea de comandos

```sh
banshee --status                  # estado en JSON
banshee --profile silent|balanced|extreme|superbatt
banshee --cycle                   # siguiente perfil (para un atajo de teclado)
```

## Aviso

Escribe registros del EC de un modelo concreto. En otro modelo puede hacer cosas inesperadas: no lo uses
sin comprobar antes el mapa del EC.

## Créditos

- **[GhostDeck](https://github.com/wygodad/ghostdeck)**, de **Dawid Wygoda** ([@wygodad](https://github.com/wygodad)):
  el proyecto original para Windows en el que se basa Banshee. De él vienen la idea y el diseño de la app,
  los perfiles, el editor de curva de ventiladores (gráficas, zonas de ruido, formas rápidas, asignación por
  perfil, escala de 0 a 150 %), el aviso de temperatura y gran parte del conocimiento del EC de MSI que usa.
  GhostDeck está publicado bajo GPL-3.0-or-later. Su nombre y su identidad visual (logo e iconos) son de su
  autor y **no** se usan aquí: el banshee de Banshee es un dibujo propio.
- **[msi-ec](https://github.com/MControlSuite/msi-ec)** (MControlSuite y colaboradores): el módulo del kernel
  que da acceso al EC desde Linux.
- **[MControlCenter](https://github.com/dmitry-s93/MControlCenter)** y **[MsiGT](https://github.com/tim-rue/MsiGT)**:
  referencia para las tablas de la curva y el cambio de modo gráfico.
- Fuentes **Chakra Petch** y **JetBrains Mono**, bajo SIL Open Font License 1.1 (ver `ui/fonts/OFL-*.txt`).

MSI y los nombres de sus productos son marcas de Micro-Star International Co., Ltd.; Banshee no tiene relación
con MSI.

## Licencia

GPL-3.0-or-later, la misma que GhostDeck. Ver [LICENSE](LICENSE).
