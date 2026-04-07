pub mod archive;
pub mod config;
pub mod dataset;
pub mod pipeline;
pub mod raw;
pub mod review;
pub mod review_payload;
pub mod types;

#[cfg(feature = "tch")]
pub mod tch_support;

pub use archive::{
    DatasetManifest, GameShard, NormalizationStats, SplitAssignment, SplitBundle, load_manifest,
    load_split_bundle, write_dataset,
};
pub use config::{AugmentationConfig, AutoCleanConfig, PipelineConfig, TeamSelector, WindowConfig};
pub use dataset::{ArchivedDataset, DatasetSource, LiveDataset, SampleIter};
pub use pipeline::{
    PipelineOutput, ReviewGame, audit_log, auto_preprocess_log, auto_preprocess_logs_with_splits,
    preprocess_log, preprocess_review_log,
};
pub use review::{ReviewStore, ReviewVerdict, load_review_store, save_review_store};
pub use review_payload::{
    ReviewGamePayload, ReviewSequencePayload, ReviewSequenceQueryPayload, build_review_payload,
    build_review_sequence_payload,
};
pub use types::{
    AuditSummary, GameMetadata, MatchPhase, ReviewSequenceSummary, RoleLabel, SequenceKind,
    TeamColor, TrainingSample, TrainingSampleMetadata,
};
