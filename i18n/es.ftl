# The texts of i18n/en.ftl in Spanish. A single line break is a space, an empty line a new
# paragraph (see src/i18n.rs).

## Stages, in the order of the chain
stage-echo = Cancelación de eco
stage-denoise = Supresión de ruido
stage-highpass = Filtro paso alto
stage-eq = Ecualizador
stage-deesser = De-esser
stage-agc = Control automático de ganancia
stage-compressor = Compresor
stage-expander = Expansor
stage-limiter = Limitador

## The groups of stages in the window
group-echo = Cancelación de eco
group-echo-description =
    Quita del micrófono el eco de los altavoces elegidos aquí, para llamadas sin auriculares.
group-denoise = Supresión de ruido
group-denoise-description =
    Quita el ruido de fondo. El límite deja una parte: más bajo suena más natural, 100 dB lo quita
    todo.
group-tone = Tono
group-tone-description =
    Quita el retumbe, hace la voz más clara (menos resonancia en los medios graves, más presencia) y
    suaviza las eses ásperas.
group-level = Nivel
group-level-description =
    Lleva la voz a un nivel constante, iguala el volumen dentro de las palabras, mantiene bajo el
    ruido en las pausas y los picos por debajo de −1 dBFS.

## Parameter titles (the daemon sends the same in English)
param-echo-protect = Protección de voz
param-denoise-level = Nivel de entrada
param-denoise-max-gain = Ganancia de entrada máxima
param-denoise-limit = Límite
param-highpass-freq = Frecuencia
param-eq-low-mid = Medios graves (250 Hz)
param-eq-presence = Presencia
param-eq-presence-freq = Frecuencia de presencia
param-deesser-freq = Frecuencia
param-deesser-threshold = Umbral
param-agc-max-gain = Ganancia máxima
param-agc-threshold = Umbral de voz
param-agc-target = Nivel objetivo
param-compressor-threshold = Umbral
param-compressor-ratio = Relación
param-expander-range = Rango
param-expander-threshold = Umbral de voz
param-expander-hold = Retención
param-expander-level = Abre por encima de
param-limiter-ceiling = Techo
param-limiter-release = Liberación
param-listen-volume = Volumen

## What the stages do, inside their rows
help-stage-echo =
    Quita del micrófono lo que reproducen los altavoces elegidos abajo, para que la otra parte de
    una llamada no se oiga a sí misma cuando no llevas auriculares. Se adapta al retardo y a la sala
    mientras suenan los altavoces.

    Añade 5 ms de latencia.
help-stage-denoise =
    Una red neuronal (DeepFilterNet3) separa la voz del ruido de fondo, como ventiladores, teclados
    o tráfico, y quita el ruido. Un micrófono bajo lo oye elevado a un nivel con el que funciona
    bien.

    Añade 30 ms de latencia, la mayor de todas las etapas, incluso desactivada (por qué: ver Filtro
    arriba del todo).
help-stage-highpass =
    Quita todo lo que está por debajo de su frecuencia: retumbe, pisadas, golpes contra la mesa y
    los chasquidos de las letras P y B. La voz en sí está por encima. Va primero, para que la
    supresión de ruido no confunda el retumbe con una voz.
help-stage-eq =
    Da forma al tono: baja los medios graves alrededor de 250 Hz, que hacen que una voz suene
    apagada o retumbante, y sube la presencia alrededor de 3,5 kHz, que hace el habla más clara.
help-stage-deesser =
    Suaviza las eses y los sonidos sibilantes ásperos: mientras la banda por encima de su frecuencia
    supone más del sonido que el umbral, baja esa banda, como mucho 12 dB. El resto de la voz queda
    como está.

    Añade 1 ms de latencia.
help-stage-agc =
    Lleva la voz a un nivel constante, hables bajo o alto, cerca del micrófono o más lejos. Se
    ajusta a lo largo de segundos (sube como mucho 6 dB por segundo, baja más rápido) y mantiene la
    ganancia en las pausas, con ruido constante como un zumbido y mientras escribes, para no
    subirlos.
help-stage-compressor =
    Iguala el volumen dentro de palabras y frases, algo demasiado rápido para el control de
    ganancia: baja las sílabas fuertes por encima del umbral y sube todo más o menos lo mismo, así
    la voz suena más uniforme y llena al mismo nivel.
help-stage-expander =
    Baja las pausas del habla, para que el ruido que queda (y que el control de ganancia subió) no
    se oiga entre palabras. Se abre en cuanto oye voz, por el nivel o por lo segura que está la
    supresión de ruido, y se cierra de nuevo tras el tiempo de retención.
help-stage-limiter =
    Mantiene los picos por debajo del techo, para que una risa, una tos o una palabra fuerte no
    saturen. Mira 1 ms por delante, para bajar antes de un pico en vez de cortarlo.

    Añade 1 ms de latencia.

## What the parameters do, behind the info button
help-param-echo-protect =
    Cuando hablas sobre lo que reproducen los altavoces, la cancelación de eco baja tu voz junto con
    el eco. Mientras hablas, se conserva esta parte de tu voz tal como era, y la supresión de ruido
    quita lo que queda del eco, sobre todo la música; mientras callas, la cancelación de eco actúa
    por completo. Más alto conserva más voz, pero deja pasar más de lo que suena por los altavoces;
    0 % lo deja solo a la cancelación de eco.
help-param-denoise-limit =
    Cuánto puede bajar el ruido. 100 dB lo quita del todo; entre 20 y 30 dB deja un poco del fondo,
    lo que suena más natural y pierde menos voz.
help-param-denoise-level =
    El nivel al que la supresión de ruido oye tu voz. Toma una voz demasiado baja por ruido y la
    silencia, así que primero sube un micrófono bajo a este nivel y luego deshace esa ganancia: lo
    que sale suena igual de fuerte. Más alto conserva más de una voz baja, pero deja pasar más ruido
    en las pausas.
help-param-denoise-max-gain =
    Cuánto puede subir un micrófono bajo para la supresión de ruido. 0 dB desactiva la subida.
help-param-highpass-freq =
    Se quita todo lo que está por debajo de esta frecuencia. Más alto quita más retumbe, pero
    adelgaza las voces graves; 80 Hz es seguro para todas las voces.
help-param-eq-low-mid =
    La ganancia alrededor de 250 Hz. Por debajo de 0 dB la voz suena menos apagada y retumbante;
    demasiado baja suena delgada.
help-param-eq-presence =
    La ganancia alrededor de la frecuencia de presencia. Por encima de 0 dB el habla suena más clara
    y cercana; demasiado alta, áspera.
help-param-eq-presence-freq =
    El centro de la banda de presencia. Más bajo suena más cálido, más alto más brillante.
help-param-deesser-freq =
    Donde empiezan las eses: baja la banda por encima de esta frecuencia. Más bajo atrapa más de
    ellas, pero también baja más de la voz.
help-param-deesser-threshold =
    La parte de la banda en el sonido total por encima de la cual actúa; 0 dB es todo el sonido. Más
    bajo actúa más a menudo y con más fuerza, más alto solo en las eses más ásperas.
help-param-agc-target =
    El nivel al que lleva la voz, en promedio (RMS). −18 dBFS es lo habitual en llamadas y deja
    margen para los picos. El compresor lo sigue.
help-param-agc-max-gain =
    Lo máximo que sube una voz baja. Más alto alcanza un habla más baja o un micrófono más lejano,
    pero también sube su ruido.
help-param-agc-threshold =
    Lo segura que debe estar la supresión de ruido de oír voz (su relación señal-ruido) para que el
    control de ganancia la mida. Más alto deja fuera más ruido, pero también partes más bajas de las
    palabras, así que sube menos la voz; −20 dB lo deja solo al nivel, como con la supresión de
    ruido desactivada.
help-param-compressor-threshold =
    Por encima de este nivel baja. Más bajo iguala más, también las sílabas normales; más alto solo
    las más fuertes.
help-param-compressor-ratio =
    Cuánto baja por encima del umbral: a 3:1, 3 dB más de entrada dan 1 dB más de salida. 1:1 no
    hace nada; más alto iguala más, pero suena aplastado.
help-param-expander-range =
    Cuánto baja las pausas. Más las hace más silenciosas, pero el cambio entre habla y pausa se nota
    más; 0 dB lo desactiva.
help-param-expander-threshold =
    Lo segura que debe estar la supresión de ruido de oír voz (su relación señal-ruido) para abrir.
    Más bajo abre más a menudo, también con ruido; más alto solo con voz clara. Con la supresión de
    ruido desactivada solo cuenta el nivel.
help-param-expander-hold =
    Cuánto tiempo sigue abierto tras la última voz. Más largo conserva los finales de palabra y las
    pausas cortas; más corto silencia las pausas antes.
help-param-expander-level =
    También abre cuando el sonido supera este nivel, para una voz baja de la que la supresión de
    ruido no está segura. Más bajo abre más a menudo, también con ruido; las pausas suelen estar
    entre −55 y −65 dBFS.
help-param-limiter-ceiling =
    Lo más alto que pueden llegar los picos. −1 dBFS deja margen para lo que añaden los códecs al
    comprimir el sonido para la llamada.
help-param-limiter-release =
    Lo rápido que suelta tras un pico. Más corto suena más fuerte, pero distorsiona antes.

## What the other rows show
help-filter =
    Activa o desactiva todo el filtro. Desactivado, las aplicaciones reciben el micrófono tal cual,
    mientras cada etapa conserva su propio interruptor y sus ajustes para cuando se active de nuevo.
    El icono de la bandeja también lo cambia.

    La latencia se mantiene aunque esté desactivado: el micrófono se retrasa tanto como tardaría el
    filtro (unos 37 ms, 30 ms de ellos la supresión de ruido; la fila Latencia de abajo lo muestra
    todo). Quitar el retraso cortaría un trozo del sonido al desactivar e insertaría un hueco al
    activar, algo que en una llamada se oye como un chasquido o un corte; así el cambio es solo un
    fundido.
help-listen-volume =
    Con qué volumen te oyes a ti mismo. Solo para la escucha: las aplicaciones reciben la voz al
    nivel al que la lleva el filtro.
help-input-level =
    Cuánto suena el micrófono antes del filtro; la barra va de −60 a 0 dBFS.

    El ruido de fondo es su momento más silencioso de los últimos 2 segundos, lo que capta mientras
    callas. Por debajo de −60 dBFS es silencioso; por encima de −45 dBFS a la supresión de ruido le
    cuesta y deja un siseo. Un realce de micrófono subido (en alsamixer suele llamarse «Mic Boost» o
    «Internal Mic Boost») sube el ruido; unos auriculares con micrófono o un micrófono USB suelen
    captar mucho menos.
help-output-level =
    Con qué volumen sale la voz del filtro, lo que reciben las aplicaciones; la barra va de −60 a 0
    dBFS. El habla debería quedar cerca del nivel objetivo del control de ganancia (−18 dBFS), las
    pausas muy por debajo.
help-latency =
    Cuánto tarda la voz del micrófono a la aplicación: el búfer del micrófono, el búfer antes del
    micrófono virtual, lo que necesitan las etapas (la supresión de ruido 30 ms) y el búfer de la
    aplicación.
help-processing =
    Cuánto tardaron 10 ms de audio en pasar por las etapas en los últimos 10 segundos: la media, el
    tiempo dentro del que quedó el 95 % y el 99 % (redondeado hacia arriba como mucho un 2 %) y el
    más largo. Todo junto, la cancelación de eco y la supresión de ruido por separado (son las que
    más tardan) y las demás etapas juntas. El tiempo más largo tiene que quedar muy por debajo de 10
    ms; si no, hay cortes. Una etapa desactivada apenas tarda nada.

    PipeWire entrega el audio en ciclos; las muestras por ciclo son su tamaño (48 muestras son 1
    ms). El filtro pide 480; las aplicaciones que necesitan menos latencia pueden reducirlo.
help-dropouts =
    Desde que arrancó el filtro.

    Silencio: audio que no estuvo listo a tiempo; el micrófono virtual reprodujo silencio en su
    lugar, un hueco. Un poco justo después de que una aplicación empiece a grabar es normal,
    mientras el búfer se asienta.

    Descartado: audio desechado para volver a bajar la latencia, por ejemplo después de que el
    filtro hiciera una pausa, un salto.

    Errores: 10 ms de audio en los que falló una etapa, sustituidos por silencio.

## The window
window-title = Voice Filter
main-menu = Menú principal
more-information = Más información
menu-reset = _Restablecer el filtro…
reset-heading = ¿Restablecer el filtro?
reset-body =
    Activa todas las etapas y pone sus ajustes en los valores predeterminados. El micrófono y los
    altavoces quedan como están.
reset-cancel = _Cancelar
reset-confirm = _Restablecer
missing-title = Voice Filter no se está ejecutando
missing-description = Inícialo con <tt>voice-filter run</tt>
missing-no-bus = No se pudo conectar al bus de sesión: { $error }
missing-no-proxy = No se pudo crear el proxy: { $error }
copy = Copiar
copied = Copiado
group-input = Entrada
group-input-description = El filtro lee de este micrófono, las aplicaciones leen de «Voice Filter».
filter = Filtro
filter-subtitle = Desactivado, las aplicaciones reciben el micrófono tal cual
microphone = Micrófono
speakers = Altavoces
default-input = Entrada predeterminada
default-output = Salida predeterminada
unplugged = { $device } (desconectado)
listen = Escuchar
listen-subtitle = Lo reproduce en la salida predeterminada, usa auriculares
listen-volume = Volumen
keep-listening = Seguir escuchando
keep-listening-subtitle =
    Vuelve a escuchar tras un reinicio. El micrófono sigue en uso y el filtro funciona todo el
    tiempo.
echo-status = Estado
echo-not-in-chain = No está en la cadena
echo-not-running = No funciona: desactivada, o ninguna aplicación graba de «Voice Filter»
echo-silent = Los altavoces están en silencio, no hay eco que quitar
echo-delay-short = menos de 4 ms
echo-delay = unos { $delay } ms
echo-hears = Oye los altavoces a { $level } dBFS, su eco vuelve tras { $delay }
default-settings = Ajustes predeterminados
reset = Restablecer
group-levels = Niveles
level-input = Micrófono
level-output = Voice Filter
input-not-running = No funciona: ninguna aplicación graba de «Voice Filter»
input-noise = Ruido de fondo a { $floor } dBFS
input-noise-loud =
    Ruido de fondo a { $floor } dBFS, muy alto: la supresión de ruido deja un siseo. Baja el realce
    del micrófono si está subido, o usa unos auriculares con micrófono
group-status = Estado
latency = Latencia
latency-value =
    { $total } ms hasta la aplicación: micrófono { $microphone }, búfer { $bridge }, procesado {
    $chain }
processing = Tiempo de procesado
processing-mean = Media
processing-p95 = 95 %
processing-p99 = 99 %
processing-longest = Máximo
processing-all = Todas las etapas
processing-other = Otras etapas
processing-summary = { $mean } ms por trama de 10 ms, máximo { $max } ms
processing-nothing = Nada procesado en los últimos 10 segundos
processing-cycle = { $samples } muestras por ciclo
processing-copied = Por trama de 10 ms, en ms: { $columns }
dropouts = Cortes desde el inicio
dropouts-value =
    { $silence } ms de silencio, { $dropped } ms descartados, { $errors ->
        [one] 1 error
       *[other] { $errors } errores
    }

## The icon in the tray
tray-title = Voice Filter
tray-on = Activado
tray-off = Desactivado: el micrófono pasa sin filtrar
tray-filter = Filtro
tray-open = Abrir Voice Filter
