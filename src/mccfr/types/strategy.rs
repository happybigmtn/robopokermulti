use crate::dto::*;
use crate::gameplay::*;
use crate::mccfr::nlhe::*;
use crate::mccfr::*;
use crate::transport::*;
use crate::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Strategy {
    info: Info,
    accumulated: BTreeMap<Edge, Probability>,
}

impl Strategy {
    pub fn info(&self) -> &Info {
        &self.info
    }
    pub fn accumulated(&self) -> &BTreeMap<Edge, Probability> {
        &self.accumulated
    }
    pub fn policy(&self) -> BTreeMap<Edge, Probability> {
        let denom = self
            .accumulated
            .values()
            .map(|&p| p.max(crate::POLICY_MIN))
            .sum::<Probability>();
        self.accumulated
            .iter()
            .map(|(&edge, &policy)| (edge, policy.max(crate::POLICY_MIN) / denom))
            .collect()
    }
}

impl Density for Strategy {
    type Support = Edge;
    fn density(&self, edge: &Self::Support) -> Probability {
        let denom = self
            .accumulated
            .values()
            .map(|&p| p.max(crate::POLICY_MIN))
            .sum::<Probability>();
        self.accumulated
            .get(edge)
            .map(|&p| p.max(crate::POLICY_MIN) / denom)
            .unwrap_or(0.)
    }
    fn support(&self) -> impl Iterator<Item = Self::Support> {
        self.accumulated.keys().copied()
    }
}

impl From<(Info, Vec<Decision>)> for Strategy {
    fn from((info, decisions): (Info, Vec<Decision>)) -> Self {
        let accumulated = info
            .choices()
            .into_iter()
            .map(|edge| {
                (
                    edge,
                    decisions
                        .iter()
                        .find(|d| d.edge == edge)
                        .map(|d| d.mass)
                        .expect("empty decision tree"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        Self { info, accumulated }
    }
}

impl TryFrom<ApiStrategy> for Strategy {
    type Error = anyhow::Error;
    fn try_from(api: ApiStrategy) -> Result<Self, Self::Error> {
        let context = InfoContext::from((
            api.seat_count as u8,
            api.seat_position as u8,
            api.active_players as u8,
        ));
        let info = Info::from((
            api.history.into(),
            api.present.into(),
            api.choices.into(),
            context,
        ));
        let accumulated = api
            .accumulated
            .into_iter()
            .map(|(edge_str, policy)| Edge::try_from(edge_str.as_str()).map(|edge| (edge, policy)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(Self { info, accumulated })
    }
}

#[cfg(test)]
mod tests {
    use super::Strategy;
    use crate::Arbitrary;
    use crate::Probability;
    use crate::gameplay::*;
    use crate::mccfr::Decision;
    use crate::mccfr::nlhe::*;
    use crate::transport::Density;

    fn build(data: &[(Action, Probability)]) -> Strategy {
        let edges = data.iter().map(|(a, _)| Edge::from(*a)).collect::<Vec<_>>();
        let accumulated = data.iter().map(|(a, p)| (Edge::from(*a), *p)).collect();
        let info = Info::from((
            Path::random(),
            Abstraction::random(),
            Path::from(edges),
            InfoContext::heads_up(),
        ));
        Strategy { info, accumulated }
    }
    fn sums(s: &Strategy) -> Probability {
        s.policy().values().sum()
    }
    fn close(a: Probability, b: Probability) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn unitarity() {
        let s = build(&[
            (Action::Fold, 10.0),
            (Action::Check, 20.0),
            (Action::Call(10), 30.0),
        ]);
        assert!(close(sums(&s), 1.0));
    }

    #[test]
    fn epsilons() {
        let s = build(&[
            (Action::Fold, 0.0),
            (Action::Check, 0.0),
            (Action::Call(10), 100.0),
        ]);
        let p = s.policy();
        assert!(close(sums(&s), 1.0));
        assert!(p[&Edge::from(Action::Call(10))] > 0.99);
    }

    #[test]
    fn clamping() {
        let s = build(&[(Action::Fold, 0.0), (Action::Check, 0.0)]);
        let p = s.policy();
        assert!(close(sums(&s), 1.0));
        assert!(close(p[&Edge::from(Action::Fold)], 0.5));
        assert!(close(p[&Edge::from(Action::Check)], 0.5));
    }

    #[test]
    fn proportionality() {
        let s = build(&[(Action::Fold, 25.0), (Action::Check, 75.0)]);
        let p = s.policy();
        assert!(close(p[&Edge::from(Action::Fold)], 0.25));
        assert!(close(p[&Edge::from(Action::Check)], 0.75));
    }

    #[test]
    fn density() {
        let s = build(&[
            (Action::Fold, 10.0),
            (Action::Check, 30.0),
            (Action::Call(10), 60.0),
        ]);
        let p = s.policy();
        for (e, v) in p.iter() {
            assert!(close(s.density(e), *v));
        }
    }

    #[test]
    fn unsupported() {
        let s = build(&[(Action::Fold, 50.0), (Action::Check, 50.0)]);
        assert_eq!(s.density(&Edge::from(Action::Call(10))), 0.0);
    }

    #[test]
    fn construction() {
        let f = Edge::from(Action::Fold);
        let c = Edge::from(Action::Check);
        let r = Edge::from(Action::Call(10));
        let i = Info::from((
            Path::random(),
            Abstraction::random(),
            Path::from(vec![f, c, r]),
            InfoContext::heads_up(),
        ));
        let d = vec![
            Decision {
                edge: f,
                mass: 10.0,
            },
            Decision {
                edge: c,
                mass: 20.0,
            },
            Decision {
                edge: r,
                mass: 70.0,
            },
        ];
        let s = Strategy::from((i.clone(), d));
        assert_eq!(s.info(), &i);
        assert_eq!(s.accumulated().len(), 3);
        assert!(close(sums(&s), 1.0));
    }

    #[test]
    fn singularity() {
        let s = build(&[(Action::Fold, 42.0)]);
        assert!(close(s.policy()[&Edge::from(Action::Fold)], 1.0));
    }

    #[test]
    fn support() {
        let s = build(&[
            (Action::Fold, 1.0),
            (Action::Check, 1.0),
            (Action::Call(10), 1.0),
        ]);
        assert_eq!(s.support().count(), 3);
    }

    #[test]
    fn positivity() {
        let s = build(&[
            (Action::Fold, 5.0),
            (Action::Check, 15.0),
            (Action::Call(10), 80.0),
        ]);
        for (_, &p) in s.policy().iter() {
            assert!(p > 0.0 && p <= 1.0);
        }
    }

    // ----- RPM-05 Required Tests -----

    /// RPM-05 AC: ApiStrategy round-trips InfoContext through serialize/deserialize.
    #[test]
    fn test_analysis_api_round_trips_info_context() {
        use crate::dto::ApiStrategy;
        let ctx = InfoContext::from((6, 4, 5));
        let info = Info::from((
            Path::from(vec![Edge::Call, Edge::Check]),
            Abstraction::from(77_i16),
            Path::from(vec![Edge::Fold, Edge::Check]),
            ctx,
        ));
        let decisions = vec![
            Decision {
                edge: Edge::Fold,
                mass: 0.4,
            },
            Decision {
                edge: Edge::Check,
                mass: 0.6,
            },
        ];
        let strategy = Strategy::from((info, decisions));
        let api: ApiStrategy = strategy.clone().into();
        // Verify context fields are present in the DTO
        assert_eq!(api.seat_count, 6);
        assert_eq!(api.seat_position, 4);
        assert_eq!(api.active_players, 5);
        // Round-trip back through TryFrom
        let recovered = Strategy::try_from(api).expect("round-trip succeeds");
        assert_eq!(recovered.info().context().seat_count(), 6);
        assert_eq!(recovered.info().context().seat_position(), 4);
        assert_eq!(recovered.info().context().active_players(), 5);
        assert_eq!(recovered.info(), strategy.info());
    }
}
