//! Read-only, authenticated addresses. The operator key ring and its
//! installation ID are never rendered or accepted over HTTP.
use super::{Failure, Lens, Moment, Result};
use crate::model::{Record, StoryTime, World};
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Read, path::Path};

const SCHEMA: &str = "wedl-compendium-keyring/v1";
const MAX_FILE: usize = 4096;
const MAX_KEYS: usize = 8;
const PAYLOAD: usize = 768;
const NONCE: usize = 12;
const TAG: usize = 16;
pub(super) const ADDRESS_LENGTH: usize = 2 * (NONCE + PAYLOAD + TAG);
pub(super) const PREFIX: &str = "/compendium/v1/";

#[derive(Clone, Copy)]
pub(crate) enum Authority {
    Fixed,
    CurrentReadOnly,
    LiveAuthor,
}

impl Authority {
    fn domain(self) -> &'static [u8] {
        match self {
            Self::Fixed => b"fixed-author",
            Self::CurrentReadOnly => b"current-read-only-author",
            Self::LiveAuthor => b"current-live-author",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema: String,
    installation: String,
    keys: Vec<String>,
}

pub(crate) struct Keyring {
    installation: [u8; 16],
    keys: Vec<LessSafeKey>,
    durable: bool,
}

fn bad_config() -> crate::Error {
    crate::Error::InvalidInput("invalid or unprotected Compendium key ring configuration".into())
}

fn exact_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut bytes = [0; N];
    hex::decode_to_slice(value, &mut bytes).ok()?;
    Some(bytes)
}

fn reparse(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

fn private_file(meta: &fs::Metadata) -> bool {
    if !meta.is_file() || reparse(meta) || meta.len() > MAX_FILE as u64 {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return false;
        }
    }
    true
}

impl Keyring {
    pub(crate) fn load(root: &Path, path: &Path) -> crate::Result<Self> {
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            || path
                .ancestors()
                .any(|part| fs::symlink_metadata(part).map_or(true, |meta| reparse(&meta)))
        {
            return Err(bad_config());
        }
        let root = root.canonicalize().map_err(|_| bad_config())?;
        let path = path.canonicalize().map_err(|_| bad_config())?;
        if path.starts_with(root)
            || !fs::symlink_metadata(&path).is_ok_and(|meta| private_file(&meta))
        {
            return Err(bad_config());
        }
        let file = fs::File::open(path).map_err(|_| bad_config())?;
        if !file.metadata().is_ok_and(|meta| private_file(&meta)) {
            return Err(bad_config());
        }
        let mut bytes = Vec::new();
        file.take((MAX_FILE + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| bad_config())?;
        if bytes.len() > MAX_FILE {
            return Err(bad_config());
        }
        let config: Config = serde_json::from_slice(&bytes).map_err(|_| bad_config())?;
        if config.schema != SCHEMA || !(1..=MAX_KEYS).contains(&config.keys.len()) {
            return Err(bad_config());
        }
        let installation = exact_hex(&config.installation).ok_or_else(bad_config)?;
        let mut distinct = BTreeSet::new();
        let mut keys = Vec::new();
        for value in config.keys {
            let key = exact_hex::<32>(&value)
                .filter(|key| distinct.insert(*key))
                .ok_or_else(bad_config)?;
            keys.push(LessSafeKey::new(
                UnboundKey::new(&CHACHA20_POLY1305, &key).map_err(|_| bad_config())?,
            ));
        }
        Ok(Self {
            installation,
            keys,
            durable: true,
        })
    }

    pub(super) fn transient() -> Result<Self> {
        let mut installation = [0; 16];
        let mut key = [0; 32];
        getrandom::fill(&mut installation).map_err(|_| Failure::Source)?;
        getrandom::fill(&mut key).map_err(|_| Failure::Source)?;
        let key = LessSafeKey::new(
            UnboundKey::new(&CHACHA20_POLY1305, &key).map_err(|_| Failure::Source)?,
        );
        Ok(Self {
            installation,
            keys: vec![key],
            durable: false,
        })
    }

    pub(super) fn durable(&self) -> bool {
        self.durable
    }

    fn binding(&self, world: &World, authority: Authority) -> [u8; 32] {
        let mut hash = Sha256::new();
        let original = match authority {
            Authority::Fixed => world
                .revision
                .as_deref()
                .unwrap_or(&world.fingerprint)
                .as_bytes(),
            _ => b"",
        };
        for field in [
            b"wedl-compendium-address-v1".as_slice(),
            &self.installation,
            authority.domain(),
            world.root.as_os_str().as_encoded_bytes(),
            original,
        ] {
            hash.update((field.len() as u64).to_be_bytes());
            hash.update(field);
        }
        hash.finalize().into()
    }

    pub(super) fn open(
        &self,
        world: &World,
        authority: Authority,
        address: &str,
    ) -> Result<Descriptor> {
        let buffer = exact_hex::<{ NONCE + PAYLOAD + TAG }>(address).ok_or(Failure::Unavailable)?;
        let (nonce, encrypted) = buffer.split_at(NONCE);
        let nonce: [u8; NONCE] = nonce.try_into().map_err(|_| Failure::Unavailable)?;
        let binding = self.binding(world, authority);
        for key in &self.keys {
            let mut packet = [0; PAYLOAD + TAG];
            packet.copy_from_slice(encrypted);
            let Ok(clear) = key.open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(binding),
                &mut packet,
            ) else {
                continue;
            };
            let length = usize::from(u16::from_be_bytes([clear[0], clear[1]]));
            if length == 0
                || length > PAYLOAD - 2
                || clear[2 + length..].iter().any(|byte| *byte != 0)
            {
                return Err(Failure::Unavailable);
            }
            return serde_json::from_slice(&clear[2..2 + length]).map_err(|_| Failure::Unavailable);
        }
        Err(Failure::Unavailable)
    }
}

fn identity(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn resolve_record<'a>(world: &'a World, digest: &str) -> Result<&'a Record> {
    let mut matches = world
        .records
        .values()
        .filter(|record| identity(&record.id) == digest);
    match (matches.next(), matches.next()) {
        (Some(record), None) => Ok(record),
        _ => Err(Failure::Unavailable),
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Member {
    Event,
    Start,
    Current,
    End,
}

impl Member {
    fn name(&self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Start => "start",
            Self::Current => "current",
            Self::End => "end",
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NamedMoment {
    timeline: String,
    owner: String,
    member: Member,
    tick: i64,
    order: i32,
}

impl NamedMoment {
    fn save(moment: &Moment) -> Result<Self> {
        let member = match moment.member {
            "event" => Member::Event,
            "start" => Member::Start,
            "current" => Member::Current,
            "end" => Member::End,
            _ => return Err(Failure::Source),
        };
        Ok(Self {
            timeline: identity(&moment.at.timeline),
            owner: identity(&moment.owner),
            member,
            tick: moment.at.tick,
            order: moment.at.order,
        })
    }

    fn resolve(self, world: &World) -> Result<Moment> {
        let owner = resolve_record(world, &self.owner)?;
        let time = owner.get("time").ok_or(Failure::Unavailable)?;
        let selected = match (&self.member, owner.kind.as_str()) {
            (Member::Event, "event") => Some(time),
            (Member::Start | Member::Current | Member::End, "scene") => {
                time.get(self.member.name())
            }
            _ => None,
        }
        .ok_or(Failure::Unavailable)?;
        let at = StoryTime::from_yaml(selected, &world.default_timeline())
            .map_err(|_| Failure::Unavailable)?;
        if identity(&at.timeline) != self.timeline || at.tick != self.tick || at.order != self.order
        {
            return Err(Failure::Unavailable);
        }
        world
            .story_time(at.tick, Some(&at.timeline), at.order)
            .map_err(|_| Failure::Unavailable)?;
        Ok(Moment {
            owner: owner.id.clone(),
            member: self.member.name(),
            at,
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "lens", rename_all = "kebab-case", deny_unknown_fields)]
enum AddressLens {
    All,
    AsOf {
        moment: NamedMoment,
    },
    Character {
        moment: NamedMoment,
        viewer: String,
        scene: Option<String>,
    },
}

impl AddressLens {
    fn save(lens: &Lens) -> Result<Self> {
        match lens {
            Lens::All => Ok(Self::All),
            Lens::AsOf(moment) => Ok(Self::AsOf {
                moment: NamedMoment::save(moment)?,
            }),
            Lens::Character {
                moment,
                viewer,
                scene,
            } => Ok(Self::Character {
                moment: NamedMoment::save(moment)?,
                viewer: identity(viewer),
                scene: scene.as_deref().map(identity),
            }),
        }
    }

    fn resolve(self, world: &World) -> Result<Lens> {
        match self {
            Self::All => Ok(Lens::All),
            Self::AsOf { moment } => {
                let moment = moment.resolve(world)?;
                let owner = &world.records[&moment.owner];
                if !crate::description::full_author_record_visible(world, owner, None)
                    .map_err(|_| Failure::Unavailable)?
                {
                    return Err(Failure::Unavailable);
                }
                Ok(Lens::AsOf(moment))
            }
            Self::Character {
                moment,
                viewer,
                scene,
            } => Ok(Lens::Character {
                moment: moment.resolve(world)?,
                viewer: resolve_record(world, &viewer)?.id.clone(),
                scene: scene
                    .as_ref()
                    .map(|scene| resolve_record(world, scene).map(|record| record.id.clone()))
                    .transpose()?,
            }),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Target<'a> {
    Directory,
    Graph,
    Record(&'a Record),
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "target", rename_all = "kebab-case", deny_unknown_fields)]
enum AddressTarget {
    Directory,
    Graph,
    Record { kind: String, record: String },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Descriptor {
    world: String,
    lens: AddressLens,
    target: AddressTarget,
}

impl Descriptor {
    pub(super) fn resolve(self, world: &World) -> Result<(Lens, Target<'_>)> {
        if self.world != identity(&world.world_record().map_err(|_| Failure::Unavailable)?.id) {
            return Err(Failure::Unavailable);
        }
        let lens = self.lens.resolve(world)?;
        let target = match self.target {
            AddressTarget::Directory => Target::Directory,
            AddressTarget::Graph => Target::Graph,
            AddressTarget::Record { kind, record } => {
                let record = resolve_record(world, &record)?;
                if record.kind != kind {
                    return Err(Failure::Unavailable);
                }
                Target::Record(record)
            }
        };
        Ok((lens, target))
    }
}

pub(super) struct Issuer<'a> {
    keyring: &'a Keyring,
    binding: [u8; 32],
    world: String,
    lens: AddressLens,
}

impl<'a> Issuer<'a> {
    pub(super) fn new(
        keyring: &'a Keyring,
        world: &World,
        authority: Authority,
        lens: &Lens,
    ) -> Result<Self> {
        Ok(Self {
            keyring,
            binding: keyring.binding(world, authority),
            world: identity(&world.world_record().map_err(|_| Failure::Source)?.id),
            lens: AddressLens::save(lens)?,
        })
    }

    pub(super) fn path(&self, target: Target<'_>) -> Result<String> {
        #[derive(Serialize)]
        struct Payload<'a> {
            world: &'a str,
            lens: &'a AddressLens,
            target: AddressTarget,
        }
        let target = match target {
            Target::Directory => AddressTarget::Directory,
            Target::Graph => AddressTarget::Graph,
            Target::Record(record) => AddressTarget::Record {
                kind: record.kind.clone(),
                record: identity(&record.id),
            },
        };
        let value = serde_json::to_vec(&Payload {
            world: &self.world,
            lens: &self.lens,
            target,
        })
        .map_err(|_| Failure::Source)?;
        if value.len() > PAYLOAD - 2 {
            return Err(Failure::Source);
        }
        let mut nonce = [0; NONCE];
        getrandom::fill(&mut nonce).map_err(|_| Failure::Source)?;
        let mut payload = [0; PAYLOAD];
        payload[..2].copy_from_slice(&(value.len() as u16).to_be_bytes());
        payload[2..2 + value.len()].copy_from_slice(&value);
        let tag = self.keyring.keys[0]
            .seal_in_place_separate_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(self.binding),
                &mut payload,
            )
            .map_err(|_| Failure::Source)?;
        let mut address = Vec::with_capacity(NONCE + PAYLOAD + TAG);
        address.extend(nonce);
        address.extend(payload);
        address.extend(tag.as_ref());
        Ok(format!("{PREFIX}{}", hex::encode(address)))
    }
}
