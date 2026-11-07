# The texts of i18n/en.ftl in Lithuanian. A single line break is a space, an empty line a new
# paragraph (see src/i18n.rs).

## Stages, in the order of the chain
stage-echo = Aido slopinimas
stage-denoise = Triukšmo slopinimas
stage-highpass = Aukštų dažnių filtras
stage-eq = Ekvalaizeris
stage-deesser = De-eseris
stage-agc = Automatinis stiprinimo reguliavimas
stage-compressor = Kompresorius
stage-expander = Ekspanderis
stage-limiter = Ribotuvas

## The groups of stages in the window
group-echo = Aido slopinimas
group-echo-description =
    Pašalina iš mikrofono čia pasirinktų garsiakalbių aidą, skambučiams be ausinių.
group-denoise = Triukšmo slopinimas
group-denoise-description =
    Pašalina foninį triukšmą. Riba palieka dalį jo: mažesnė skamba natūraliau, 100 dB pašalina
    viską.
group-tone = Tembras
group-tone-description =
    Pašalina ūžesį, daro balsą aiškesnį (mažiau bumbėjimo žemuosiuose vidutiniuose, daugiau artumo)
    ir sušvelnina aštrius S garsus.
group-level = Lygis
group-level-description =
    Suveda balsą į pastovų lygį, išlygina garsumą žodžių viduje, kalbos pauzėse laiko triukšmą
    žemai, o pikus žemiau −1 dBFS.

## Parameter titles (the daemon sends the same in English)
param-echo-protect = Balso apsauga
param-denoise-level = Įėjimo lygis
param-denoise-max-gain = Didžiausias įėjimo stiprinimas
param-denoise-limit = Riba
param-highpass-freq = Dažnis
param-eq-low-mid = Žemieji vidutiniai (250 Hz)
param-eq-presence = Artumas
param-eq-presence-freq = Artumo dažnis
param-deesser-freq = Dažnis
param-deesser-threshold = Slenkstis
param-agc-max-gain = Didžiausias stiprinimas
param-agc-threshold = Kalbos slenkstis
param-agc-target = Tikslinis lygis
param-compressor-threshold = Slenkstis
param-compressor-ratio = Santykis
param-expander-range = Gylis
param-expander-threshold = Kalbos slenkstis
param-expander-hold = Išlaikymas
param-expander-level = Atsidaro virš
param-limiter-ceiling = Lubos
param-limiter-release = Atleidimas
param-listen-volume = Garsumas

## What the stages do, inside their rows
help-stage-echo =
    Pašalina iš mikrofono tai, ką groja žemiau pasirinkti garsiakalbiai, kad pašnekovas skambučio
    metu negirdėtų savęs, kai nedėvi ausinių. Prisitaiko prie vėlavimo ir patalpos, kol
    garsiakalbiai groja.

    Prideda 5 ms delsos.
help-stage-denoise =
    Neuroninis tinklas (DeepFilterNet3) atskiria balsą nuo foninio triukšmo, pavyzdžiui,
    ventiliatorių, klaviatūrų ar eismo, ir pašalina triukšmą. Tylų mikrofoną jis girdi pakeltą iki
    lygio, kuriame gerai veikia.

    Prideda 30 ms delsos, daugiausia iš visų etapų, net kai išjungtas (kodėl – žr. „Filtras“
    viršuje).
help-stage-highpass =
    Pašalina viską, kas žemiau jo dažnio: ūžesį, žingsnius, stuktelėjimus į stalą ir P bei B garsų
    pokštelėjimus. Pats balsas yra aukščiau. Jis eina pirmas, kad triukšmo slopinimas nepalaikytų
    ūžesio balsu.
help-stage-eq =
    Formuoja tembrą: pritildo žemuosius vidutinius dažnius apie 250 Hz, dėl kurių balsas skamba
    duslai ar bumbiai, ir pakelia artumą apie 3,5 kHz, dėl kurio kalba aiškesnė.
help-stage-deesser =
    Pritildo aštrius S ir Š garsus: kol juosta virš jo dažnio sudaro didesnę garso dalį nei
    slenkstis, jis tą juostą pritildo, daugiausia 12 dB. Likęs balsas lieka toks, koks buvo.

    Prideda 1 ms delsos.
help-stage-agc =
    Suveda balsą į pastovų lygį, nesvarbu, ar kalbi tyliai, ar garsiai, arti mikrofono ar toliau.
    Prisitaiko per kelias sekundes (aukštyn daugiausia 6 dB per sekundę, žemyn greičiau), o kalbos
    pauzėse esant pastoviam triukšmui, pvz., ūžimui, ir kol rašai klaviatūra, išlaiko stiprinimą,
    kad jų nepakeltų.
help-stage-compressor =
    Išlygina garsumą žodžių ir sakinių viduje, o tai stiprinimo reguliavimui per greita: virš
    slenksčio jis pritildo garsius skiemenis ir maždaug tiek pat pakelia viską, todėl tuo pačiu
    lygiu balsas skamba tolygiau ir sodriau.
help-stage-expander =
    Pritildo kalbos pauzes, kad likęs triukšmas (ir tas, kurį pakėlė stiprinimo reguliavimas) nebūtų
    girdimas tarp žodžių. Atsidaro, vos išgirdęs kalbą – pagal lygį arba pagal tai, kiek triukšmo
    slopinimas tikras, – ir vėl užsidaro praėjus išlaikymo laikui.
help-stage-limiter =
    Laiko pikus žemiau lubų, kad juokas, kosulys ar garsus žodis neperkrautų signalo. Jis žiūri 1 ms
    į priekį, kad pritildytų prieš piką, o ne jį nukirstų.

    Prideda 1 ms delsos.

## What the parameters do, behind the info button
help-param-echo-protect =
    Kai kalbi kartu su tuo, ką groja garsiakalbiai, aido slopinimas pritildo tavo balsą kartu su
    aidu. Kol kalbi, tiek tavo balso lieka nepakeista, o likusį aidą, ypač muziką, pašalina triukšmo
    slopinimas; kai tyli, aido slopinimas veikia visa jėga. Daugiau išsaugo daugiau balso, bet po
    juo praleidžia daugiau to, ką groja garsiakalbiai; 0 % palieka viską vien aido slopinimui.
help-param-denoise-limit =
    Kiek jis gali pritildyti triukšmą. 100 dB jį pašalina visiškai; apie 20–30 dB palieka šiek tiek
    fono, o tai skamba natūraliau ir mažiau praranda balso.
help-param-denoise-level =
    Lygis, kuriuo triukšmo slopinimas girdi tavo balsą. Per tylų balsą jis palaiko triukšmu ir
    nutildo, todėl tylų mikrofoną pirmiausia pakelia iki šio lygio, o po jo stiprinimą atima: tai,
    kas išeina, lieka tokio pat garsumo. Didesnis išsaugo daugiau tylaus balso, bet pauzėse
    praleidžia daugiau triukšmo.
help-param-denoise-max-gain =
    Kiek daugiausia jis gali pakelti tylų mikrofoną triukšmo slopinimui. 0 dB pakėlimą išjungia.
help-param-highpass-freq =
    Viskas, kas žemiau šio dažnio, pašalinama. Didesnis pašalina daugiau ūžesio, bet žemus balsus
    daro plonesnius; 80 Hz tinka visiems balsams.
help-param-eq-low-mid =
    Stiprinimas apie 250 Hz. Žemiau 0 dB balsas skamba mažiau duslai ir bumbiai, per daug
    pritildžius – plonai.
help-param-eq-presence =
    Stiprinimas apie artumo dažnį. Virš 0 dB kalba skamba aiškiau ir arčiau, per daug pakėlus –
    aštriai.
help-param-eq-presence-freq =
    Artumo juostos centras. Žemesnis skamba šilčiau, aukštesnis – šviesiau.
help-param-deesser-freq =
    Kur prasideda S garsai: jis pritildo juostą virš šio dažnio. Žemesnis pagauna daugiau jų, bet
    pritildo ir daugiau balso.
help-param-deesser-threshold =
    Juostos dalis visame garse, virš kurios jis veikia; 0 dB – tai visas garsas. Žemesnis veikia
    dažniau ir stipriau, aukštesnis – tik esant aštriausiems S garsams.
help-param-agc-target =
    Lygis, iki kurio jis suveda balsą, vidutiniškai (RMS). −18 dBFS įprasta skambučiams ir palieka
    vietos pikams. Kompresorius jį seka.
help-param-agc-max-gain =
    Kiek daugiausia jis pakelia tylų balsą. Didesnis pasiekia tylesnę kalbą ar toliau esantį
    mikrofoną, bet pakelia ir jo triukšmą.
help-param-agc-threshold =
    Kiek triukšmo slopinimas turi būti tikras, kad girdi kalbą (jo signalo ir triukšmo santykis),
    kad stiprinimo reguliavimas ją matuotų. Didesnis neįtraukia daugiau triukšmo, bet ir tylesnių
    žodžių dalių, todėl balsą pakelia mažiau; −20 dB palieka viską vien lygiui, kaip išjungus
    triukšmo slopinimą.
help-param-compressor-threshold =
    Virš šio lygio jis pritildo. Žemesnis išlygina daugiau, ir įprastus skiemenis, aukštesnis – tik
    garsiausius.
help-param-compressor-ratio =
    Kiek jis pritildo virš slenksčio: esant 3:1, 3 dB daugiau įėjime duoda 1 dB daugiau išėjime. 1:1
    nieko nedaro, didesnis išlygina daugiau, bet skamba suspaustai.
help-param-expander-range =
    Kiek jis pritildo pauzes. Daugiau padaro jas tylesnes, bet perėjimas tarp kalbos ir pauzės
    girdimas labiau; 0 dB jį išjungia.
help-param-expander-threshold =
    Kiek triukšmo slopinimas turi būti tikras, kad girdi kalbą (jo signalo ir triukšmo santykis),
    kad jis atsidarytų. Žemesnis atsidaro dažniau, ir triukšmui, aukštesnis – tik aiškiai kalbai.
    Išjungus triukšmo slopinimą, svarbus tik lygis.
help-param-expander-hold =
    Kiek ilgai jis lieka atidarytas po paskutinės kalbos. Ilgesnis išsaugo žodžių galus ir trumpas
    pauzes, trumpesnis greičiau nutildo pauzes.
help-param-expander-level =
    Jis taip pat atsidaro, kai garsas tampa garsesnis už šį lygį, – tyliai kalbai, dėl kurios
    triukšmo slopinimas nėra tikras. Žemesnis atsidaro dažniau, ir triukšmui; pauzės paprastai būna
    nuo −55 iki −65 dBFS.
help-param-limiter-ceiling =
    Kiek daugiausia gali pakilti pikai. −1 dBFS palieka vietos tam, ką prideda kodekai, kai
    suspaudžia garsą skambučiui.
help-param-limiter-release =
    Kaip greitai jis atleidžia po piko. Trumpesnis skamba garsiau, bet greičiau iškraipo.

## What the other rows show
help-filter =
    Įjungia ir išjungia visą filtrą. Išjungus programos gauna mikrofoną tokį, koks jis yra, o
    kiekvienas etapas išlaiko savo jungiklį ir nustatymus tam laikui, kai filtras vėl bus įjungtas.
    Jį perjungia ir piktograma sistemos dėkle.

    Delsa išlieka ta pati, kai jis išjungtas: mikrofonas vėlinamas tiek, kiek užtruktų filtras (apie
    37 ms, iš jų 30 ms – triukšmo slopinimas; visą delsą rodo žemiau esanti eilutė „Delsa“).
    Atsisakius vėlinimo, išjungiant būtų iškirpta garso dalis, o įjungiant įterptas tarpas,
    skambutyje girdimas kaip spragtelėjimas ar strigimas; taip perjungimas tik sklandžiai pereina.
help-listen-volume =
    Kaip garsiai girdi save. Tik klausymuisi: programos gauna balsą tokiu lygiu, iki kurio jį suveda
    filtras.
help-input-level =
    Kaip garsus mikrofonas prieš filtrą; juosta rodo nuo −60 iki 0 dBFS.

    Foninis triukšmas – tai tyliausia akimirka per paskutines 2 sekundes, tai, ką jis pagauna, kai
    tyli. Žemiau −60 dBFS – tylu; virš −45 dBFS triukšmo slopinimui sunku ir lieka šnypštimas.
    Padidintas mikrofono stiprinimas (alsamixer dažnai „Mic Boost“ arba „Internal Mic Boost“)
    padidina triukšmą; ausinės su mikrofonu ar USB mikrofonas paprastai pagauna daug mažiau.
help-output-level =
    Kaip garsiai balsas išeina iš filtro, tai, ką gauna programos; juosta rodo nuo −60 iki 0 dBFS.
    Kalba turėtų būti apie stiprinimo reguliavimo tikslinį lygį (−18 dBFS), pauzės – gerokai žemiau.
help-latency =
    Kiek laiko balsas keliauja nuo mikrofono iki programos: mikrofono buferis, buferis prieš
    virtualų mikrofoną, ko reikia etapams (triukšmo slopinimui 30 ms) ir programos buferis.
help-processing =
    Kiek laiko per paskutines 10 sekundžių užtruko 10 ms garso apdorojimas etapuose: vidurkis,
    laikas, į kurį tilpo 95 % ir 99 % jo (suapvalintas į didesnę pusę daugiausia 2 %), ir
    ilgiausias. Visi etapai kartu, aido slopinimas ir triukšmo slopinimas atskirai (jie užima
    daugiausia) ir kiti etapai kartu. Ilgiausias laikas turi likti gerokai žemiau 10 ms, kitaip
    atsiranda trūkinėjimų; išjungtas etapas laiko beveik neužima.

    PipeWire perduoda garsą ciklais, mėginių skaičius per ciklą yra jų dydis (48 mėginiai – 1 ms).
    Filtras prašo 480, mažesnės delsos reikalaujančios programos gali jį sumažinti.
help-dropouts =
    Nuo filtro paleidimo.

    Tyla: garsas, kuris nebuvo paruoštas laiku, virtualus mikrofonas vietoj jo grojo tylą – tarpas.
    Šiek tiek jos iškart po to, kai programa pradeda įrašinėti, yra normalu, kol buferis nusistovi.

    Atmesta: garsas, išmestas, kad delsa vėl sumažėtų, pvz., po filtro pauzės – šuolis.

    Klaidos: 10 ms garso, kurio etapas neapdorojo, pakeista tyla.

## The window
window-title = Voice Filter
main-menu = Pagrindinis meniu
more-information = Daugiau informacijos
menu-reset = _Atstatyti filtrą…
reset-heading = Atstatyti filtrą?
reset-body =
    Įjungia visus etapus ir grąžina jų numatytuosius nustatymus. Mikrofonas ir garsiakalbiai lieka
    nepakeisti.
reset-cancel = _Atšaukti
reset-confirm = _Atstatyti
missing-title = Voice Filter neveikia
missing-description = Paleisk jį komanda <tt>voice-filter run</tt>
missing-no-bus = Nepavyko prisijungti prie seanso magistralės: { $error }
missing-no-proxy = Nepavyko sukurti tarpinio objekto: { $error }
copy = Kopijuoti
copied = Nukopijuota
group-input = Įvestis
group-input-description = Filtras skaito iš šio mikrofono, programos skaito iš „Voice Filter“.
filter = Filtras
filter-subtitle = Išjungus programos gauna mikrofoną tokį, koks jis yra
microphone = Mikrofonas
speakers = Garsiakalbiai
default-input = Numatytoji įvestis
default-output = Numatytoji išvestis
unplugged = { $device } (atjungtas)
listen = Klausytis
listen-subtitle = Groja per numatytąją išvestį, naudok ausines
listen-volume = Garsumas
keep-listening = Klausytis toliau
keep-listening-subtitle =
    Po paleidimo iš naujo vėl klausosi. Mikrofonas lieka naudojamas, o filtras veikia visą laiką.
echo-status = Būsena
echo-not-in-chain = Nėra grandinėje
echo-not-running = Neveikia: išjungta arba jokia programa neįrašinėja iš „Voice Filter“
echo-silent = Garsiakalbiai tyli, nėra aido, kurį reikėtų pašalinti
echo-delay-short = mažiau nei 4 ms
echo-delay = apie { $delay } ms
echo-hears = Girdi garsiakalbius { $level } dBFS lygiu, jų aidas grįžta po { $delay }
default-settings = Numatytieji nustatymai
reset = Atstatyti
group-levels = Lygiai
level-input = Mikrofonas
level-output = Voice Filter
input-not-running = Neveikia: jokia programa neįrašinėja iš „Voice Filter“
input-noise = Foninis triukšmas { $floor } dBFS
input-noise-loud =
    Foninis triukšmas { $floor } dBFS, labai garsus: triukšmo slopinimas palieka šnypštimą. Sumažink
    mikrofono stiprinimą, jei jis padidintas, arba naudok ausines su mikrofonu
group-status = Būsena
latency = Delsa
latency-value =
    { $total } ms iki programos: mikrofonas { $microphone }, buferis { $bridge }, apdorojimas {
    $chain }
processing = Apdorojimo laikas
processing-mean = Vidurkis
processing-p95 = 95 %
processing-p99 = 99 %
processing-longest = Ilgiausias
processing-all = Visi etapai
processing-other = Kiti etapai
processing-summary = { $mean } ms per 10 ms kadrą, ilgiausias { $max } ms
processing-nothing = Per paskutines 10 sekundžių nieko neapdorota
processing-cycle =
    { $samples } { $samples ->
        [one] mėginys
        [few] mėginiai
       *[other] mėginių
    } per ciklą
processing-copied = Per 10 ms kadrą, ms: { $columns }
dropouts = Trūkinėjimai nuo paleidimo
dropouts-value =
    { $silence } ms tylos, { $dropped } ms atmesta, { $errors ->
        [one] { $errors } klaida
        [few] { $errors } klaidos
       *[other] { $errors } klaidų
    }

## The icon in the tray
tray-title = Voice Filter
tray-on = Įjungta
tray-off = Išjungta: mikrofonas perduodamas nefiltruotas
tray-filter = Filtras
tray-open = Atidaryti Voice Filter
