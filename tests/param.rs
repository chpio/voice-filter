use ::voice_filter::agc::Agc;
use ::voice_filter::chain::Chain;
use ::voice_filter::compressor::Compressor;
use ::voice_filter::limiter::Limiter;
use ::voice_filter::param::Param;

#[test]
fn keeps_its_value_within_the_range_and_shares_it_with_its_clones() {
    let param = Param::new("freq", "Frequency", "Hz", (40., 200., 5.), 80.);
    assert_eq!(param.get(), 80.);
    let clone = param.clone();
    clone.set(120.).unwrap();
    assert_eq!(param.get(), 120.);
    for wrong in [39., 201., f32::NAN] {
        assert!(param.set(wrong).is_err());
    }
    assert_eq!(param.get(), 120.);
    // the bounds belong to the range
    param.set(40.).unwrap();
    param.set(200.).unwrap();
}

#[test]
fn the_chain_lists_the_parameters_of_its_stages_in_order() {
    let agc = Agc::new();
    let compressor = Compressor::new().following(agc.target());
    let chain = Chain::new(vec![
        Box::new(agc),
        Box::new(compressor),
        Box::new(Limiter::new()),
    ]);
    let names: Vec<_> = chain
        .params()
        .iter()
        .map(|(stage, param)| (*stage, param.name))
        .collect();
    assert_eq!(
        names,
        [
            ("agc", "target"),
            ("agc", "max-gain"),
            ("agc", "threshold"),
            ("compressor", "threshold"),
            ("compressor", "ratio"),
            ("limiter", "ceiling"),
            ("limiter", "release"),
        ]
    );
}
