use crate::db::types::Blake3Hash;
use crate::db::types::FileNodeType;
use anyhow::Context as _;
use chrono::{DateTime, Utc};
use collections::FxIndexMap;
use creek::ReadDiskStream;
use creek::SymphoniaDecoder;
use crossbeam_channel::Sender;
use lofty::file::AudioFile as _;
use lofty::file::TaggedFileExt as _;
use lofty::probe::Probe;
use lofty::tag::Accessor as _;
use lofty::tag::ItemKey;
use orx_tree::DynTree;
use orx_tree::NodeRef;
use orx_tree::Traversal;
use orx_tree::Traverser as _;
use rkyv::string::ArchivedString;
use std::collections::HashMap;
use std::env;
use std::os::unix::fs::MetadataExt as _;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::sync::Arc;
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
pub struct Track {
    pub id: Uuid,
    pub artist: String, // Arc<str>???
    pub title: String,
    pub filepath: String,
}

pub struct PersistenceState {
    db: sqlx::Pool<sqlx::Sqlite>,
}
impl PersistenceState {
    async fn handle_fetch_library(
        &mut self,
        reply: flume::Sender<FetchLibraryRes>,
    ) -> anyhow::Result<()> {
        let db_libraries = sqlx::query_as!(
            DbLibrary,
            r#"SELECT id as "id: Uuid", path, node as "node: Uuid" FROM libraries"#
        )
        .fetch_all(&self.db)
        .await?;

        let mut library_snapshot = FxIndexMap::default();
        let mut filenodes_tracks = vec![];

        for l in &db_libraries {
            if l.node.is_none() {
                continue;
            }

            let recs = sqlx::query!(
                r#"
                WITH RECURSIVE filenodes_tree AS (
                    SELECT
                        f.id,
                        f.inode,
                        f.device,
                        f.parent_id,
                        f.mtime,
                        f.size,
                        f.node_type,
                        f.name,
                        l.path as path
                    FROM filenodes f
                    JOIN libraries l
                        ON l.node = f.id
                    WHERE f.id = ?

                    UNION ALL

                    SELECT
                        f.id,
                        f.inode,
                        f.device,
                        f.parent_id,
                        f.mtime,
                        f.size,
                        f.node_type,
                        f.name,
                        (t.path || '/' || f.name) as path
                    FROM filenodes f
                    JOIN filenodes_tree t ON f.parent_id = t.id
                )
                SELECT
                    fn.id as "id: Uuid",
                    fn.inode as "inode: u64",
                    fn.device as "device: u64",
                    fn.parent_id as "parent_id: Uuid",
                    fn.mtime as "mtime: DateTime<Utc>",
                    fn.size as "size: u64",
                    fn.node_type as "node_type: FileNodeType",
                    fn.name as "name: String",
                    fn.path as "path: Arc<str>",
                    t.id AS "track_id: Uuid",
                    t.artist,
                    t.title
                FROM filenodes_tree fn
                INNER JOIN tracks t
                    ON t.filenode_id == fn.id;
                "#,
                l.node,
            )
            .fetch_all(&self.db)
            .await?;

            library_snapshot.reserve(recs.len());
            filenodes_tracks.reserve(recs.len());

            for rec in recs {
                let db_file_node = DbFileNode {
                    id: rec.id.unwrap(),
                    inode: rec.inode.unwrap(),
                    device: rec.device.unwrap(),
                    parent_id: rec.parent_id,
                    name: rec.name.unwrap(),
                    mtime: rec.mtime.unwrap(),
                    size: rec.size.unwrap(),
                    node_type: rec.node_type.unwrap(),
                };
                let track = Arc::new(Track {
                    id: rec.track_id,
                    artist: rec.artist.unwrap_or_else(|| String::new()),
                    title: rec.title.unwrap_or_else(|| String::new()),
                    filepath: rec.path.unwrap().to_string(),
                });

                library_snapshot.insert(track.id, track.clone());
                filenodes_tracks.push((db_file_node, track));
            }
        }

        reply
            .try_send(FetchLibraryRes::Snapshot(library_snapshot))
            .map_err(|e| anyhow::anyhow!("FetchLibraryRes::Snapshot: {:#?}", e))?;

        // let mut tx = self.db.begin().await?;
        //
        // let mut file_nodes_state = FileNodesState::new(filenodes_tracks);
        //
        // for l in db_libraries {
        //     println!("library_id={}", l.id);
        //
        //     let library_state = file_nodes_state.create_library_state(l)?;
        //     // println!("fs_tree={:#?}", ());
        //
        //     // Uncommenting triggers this
        //     //
        //     // 1. rustc: lifetime bound not satisfied
        //     //    this is a known limitation that will be removed in the future (see issue #100013 <https://github.com/rust-lang/rust/issues/100013> for more information)
        //     // 2. rustc: implementation of `orx_tree::node_ref::NodeRefCore` is not general enough
        //     //    `orx_tree::node_ref::NodeRefCore<'_, orx_tree::Dyn<library::FsNode>, orx_tree::Auto, orx_tree::pinned_storage::SplitRecursive>` would have to be implemented for the type `orx_tree::Node<'_, orx_tree::Dyn<library::FsNode>>`
        //     //    ...but `orx_tree::node_ref::NodeRefCore<'0, orx_tree::Dyn<library::FsNode>, orx_tree::Auto, orx_tree::pinned_storage::SplitRecursive>` is actually implemented for the type `orx_tree::Node<'0, orx_tree::Dyn<library::FsNode>>`, for some specific lifetime `'0`
        //     //
        //     // due to the `sqlx::query!()` calls inside the function,
        //     // but not as this callsitte but in `rt.spawn(io_thread(...))` function call in the  `audio_handle.rs` file. Strange...
        //     // library_state.sync_with_db(&mut *tx).await?;
        // }
        //
        // tx.commit().await?;

        Ok(())
    }
}

pub struct DbLibrary {
    pub id: Uuid,
    pub path: PathBuf,
    pub node: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct DbFileNode {
    pub id: Uuid,
    pub inode: u64,
    pub device: u64,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub mtime: DateTime<Utc>,
    pub size: u64,
    pub node_type: FileNodeType,
    // audio_hash: Option<Blake3Hash>,
    // meta_hash: Option<Blake3Hash>,
    // node_hash: Option<Blake3Hash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub inode: u64,
    pub device: u64, // Always check device + inode together
}

#[derive(Debug, PartialEq, Eq)]
pub enum FsFileType {
    Directory,
    AudioFile {
        artist: Option<String>,
        title: Option<String>,
    },
}

impl From<&FsFileType> for FileNodeType {
    fn from(value: &FsFileType) -> Self {
        match value {
            FsFileType::Directory => FileNodeType::Directory,
            FsFileType::AudioFile { .. } => FileNodeType::File,
        }
    }
}

#[derive(Debug)]
pub struct FsNode {
    pub path: PathBuf,
    pub file_type: FsFileType,
    pub identity: FileIdentity,
    pub size: u64,
    pub mtime: DateTime<Utc>,
    pub parent_id: Option<Uuid>,

    pub db_id: Uuid,
    pub op: SyncOp,
}

impl FsNode {
    pub async fn insert_into_db(
        &self,
        connection: &mut sqlx::SqliteConnection,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO filenodes (id, inode, device, parent_id, name, mtime, size, node_type)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?);
            "#,
            self.db_id,
            self.identity.inode as i64,
            self.identity.device as i64,
            self.parent_id,
            self.path
                .file_name()
                .expect("Invalid Dir Path")
                .to_string_lossy()
                .to_string(),
            self.mtime,
            self.size as i64,
            Into::<FileNodeType>::into(&self.file_type),
        )
        .execute(&mut *connection)
        .await
        .context("filenodes insert failed")?;

        match &self.file_type {
            FsFileType::Directory => {}
            FsFileType::AudioFile { artist, title } => {
                sqlx::query!(
                    r#"
                    INSERT INTO tracks (id, filenode_id, artist, title)
                    VALUES (?, ?, ?, ?);
                    "#,
                    Uuid::new_v4(),
                    self.db_id,
                    artist,
                    title
                )
                .execute(&mut *connection)
                .await
                .context("tracks insert failed")?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SyncOp {
    Synced,
    UpdateMeta,
    Insert,
    Move { old_parent_id: Option<Uuid> },
    NewRoot { library_id: Uuid },
}

fn get_identity(meta: &std::fs::Metadata) -> FileIdentity {
    #[cfg(unix)]
    {
        FileIdentity {
            inode: meta.ino(),
            device: meta.dev(),
        }
    }
    #[cfg(windows)]
    {
        // Windows 'volume_serial_number' acts as device ID
        FileIdentity {
            inode: meta.file_index().unwrap_or(0),
            device: meta.volume_serial_number().unwrap_or(0) as u64,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        FileIdentity {
            inode: 0,
            device: 0,
        }
    }
}

pub struct FileNodesState {
    pub entries: HashMap<Uuid, (DbFileNode, Arc<Track>)>,
    pub children_map: HashMap<Uuid, Vec<Uuid>>,
    pub identity_map: HashMap<FileIdentity, Uuid>,
}

impl FileNodesState {
    pub fn new(filenodes_tracks: Vec<(DbFileNode, Arc<Track>)>) -> Self {
        let len = filenodes_tracks.len();

        let mut entries = HashMap::with_capacity(len);
        let mut children_map = HashMap::with_capacity(len);
        let mut identity_map = HashMap::with_capacity(len);

        for filenode_track in filenodes_tracks {
            if let Some(parent_id) = filenode_track.0.parent_id {
                children_map
                    .entry(parent_id)
                    .and_modify(|ids: &mut Vec<_>| ids.push(filenode_track.0.id))
                    .or_insert_with(|| vec![filenode_track.0.id]);
            }
            identity_map.insert(
                FileIdentity {
                    inode: filenode_track.0.inode,
                    device: filenode_track.0.device,
                },
                filenode_track.0.id,
            );
            entries.insert(filenode_track.0.id, filenode_track);
        }

        Self {
            entries,
            children_map,
            identity_map,
        }
    }

    pub fn create_library_state(&mut self, l: DbLibrary) -> anyhow::Result<LibraryState> {
        let (root_id, op) = l
            .node
            .map(|id| (id, SyncOp::Synced))
            .unwrap_or((Uuid::new_v4(), SyncOp::NewRoot { library_id: l.id }));

        let meta = l.path.metadata().context("Unable to read file metadata")?;
        assert!(meta.is_dir());

        let mut path_map = HashMap::new();
        let mut identity_map = HashMap::new();

        let mut stack = vec![root_id];

        while let Some(id) = stack.pop() {
            let Some(children) = self.children_map.get(&id) else {
                continue;
            };

            for &child_id in children {
                let Some(child_filenode_track) = self.entries.get(&child_id) else {
                    continue;
                };

                path_map.insert(
                    (
                        child_filenode_track.0.parent_id,
                        child_filenode_track.0.name.clone(),
                    ),
                    child_id,
                );
                identity_map.insert(
                    FileIdentity {
                        inode: child_filenode_track.0.inode,
                        device: child_filenode_track.0.device,
                    },
                    child_id,
                );

                stack.push(child_id);
            }
        }

        let mut tree = DynTree::new(FsNode {
            db_id: root_id,
            path: l.path,
            file_type: FsFileType::Directory,
            identity: FileIdentity {
                inode: meta.ino(),
                device: meta.dev(),
            },
            size: meta.size(),
            mtime: DateTime::<Utc>::from(meta.modified()?),
            parent_id: None,
            op,
        });
        let mut stack = vec![tree.root().idx()];

        for entry in WalkDir::new(&tree.root().data().path)
            .min_depth(1)
            .sort_by_file_name()
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    log::info!("Unable to read entry: {:#?}", entry);
                    continue;
                }
            };

            let file_type = entry.file_type();

            if file_type.is_file()
                && !entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| {
                        matches!(
                            ext.to_lowercase().as_str(),
                            "mp3" | "flac" | "wav" | "m4a" | "ogg"
                        )
                    })
                    .unwrap_or(false)
            {
                continue;
            }

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => {
                    log::info!("Unable to read entry metadata: {:#?}", entry);
                    continue;
                }
            };

            let is_dir = meta.is_dir();
            let name = entry.file_name().to_string_lossy().to_string();
            let depth = entry.depth();
            let identity = get_identity(&meta);
            let mtime = DateTime::<Utc>::from(meta.modified()?);
            let size = meta.len();
            let file_type = if file_type.is_dir() {
                FsFileType::Directory
            } else {
                let tagged_file = Probe::open(entry.path())?
                    .read()
                    .expect("ERROR: Failed to read file!");

                // let tag = match tagged_file.primary_tag() {
                //     Some(primary_tag) => primary_tag,
                //     // If the "primary" tag doesn't exist, we just grab the
                //     // first tag we can find. Realistically, a tag reader would likely
                //     // iterate through the tags to find a suitable one.
                //     None => tagged_file
                //         .first_tag()
                //         .context("Unable to read first tag")?,
                // };

                let tag = tagged_file.primary_tag().or(tagged_file.first_tag());

                if let Some(tag) = tag {
                    // println!("--- Tag Information ---");
                    // println!("Title: {}", tag.title().as_deref().unwrap_or("None"));
                    // println!("Artist: {}", tag.artist().as_deref().unwrap_or("None"));
                    // println!("Album: {}", tag.album().as_deref().unwrap_or("None"));
                    // println!("Genre: {}", tag.genre().as_deref().unwrap_or("None"));
                    //
                    let artist = tag.artist().as_deref().unwrap_or("").to_string();
                    let title = tag.title().as_deref().unwrap_or("").to_string();

                    // // import keys from https://docs.rs/lofty/latest/lofty/tag/enum.ItemKey.html
                    // println!(
                    //     "Album Artist: {}",
                    //     tag.get_string(&ItemKey::AlbumArtist).unwrap_or("None")
                    // );

                    // let properties = tagged_file.properties();
                    //
                    // let duration = properties.duration();
                    // let seconds = duration.as_secs() % 60;
                    //
                    // let duration_display =
                    //     format!("{:02}:{:02}", (duration.as_secs() - seconds) / 60, seconds);
                    //
                    // println!("--- Audio Properties ---");
                    // println!(
                    //     "Bitrate (Audio): {}",
                    //     properties.audio_bitrate().unwrap_or(0)
                    // );
                    // println!(
                    //     "Bitrate (Overall): {}",
                    //     properties.overall_bitrate().unwrap_or(0)
                    // );
                    // println!("Sample Rate: {}", properties.sample_rate().unwrap_or(0));
                    // println!("Bit depth: {}", properties.bit_depth().unwrap_or(0));
                    // println!("Channels: {}", properties.channels().unwrap_or(0));
                    // println!("Duration: {duration_display}");

                    println!(
                        "found tag for {:#?}, {:#?} - {:#?}",
                        entry.path(),
                        artist,
                        title
                    );

                    FsFileType::AudioFile {
                        artist: Some(artist),
                        title: Some(title),
                    }
                } else {
                    println!("not found tag for {:#?}", entry.path());
                    FsFileType::AudioFile {
                        artist: None,
                        title: None,
                    }
                }
            };

            let parent_idx = stack[depth - 1];
            let parent_id = tree.node(parent_idx).data().db_id;

            let (node_id, op) = if let Some(id) = path_map.remove(&(Some(parent_id), name)) {
                let filenode_track = self.entries.get(&id).unwrap();

                let filenode_identity = FileIdentity {
                    inode: filenode_track.0.inode,
                    device: filenode_track.0.device,
                };
                identity_map.remove(&filenode_identity);

                if filenode_identity != identity
                    || filenode_track.0.mtime != mtime
                    || filenode_track.0.size != size
                {
                    (filenode_track.0.id, SyncOp::UpdateMeta)
                } else {
                    (filenode_track.0.id, SyncOp::Synced)
                }
            } else if let Some(existing_id) = identity_map.remove(&identity) {
                let filenode_track = &self.entries[&existing_id];

                println!(
                    "Detected Move (Inode): {:#?} -> {:#?}",
                    entry.path(),
                    tree.node(parent_idx).data().path
                );
                (
                    existing_id,
                    SyncOp::Move {
                        old_parent_id: filenode_track.0.parent_id,
                    },
                )
            } else if let Some(&existing_id) = self.identity_map.get(&identity) {
                let filenode_track = &self.entries[&existing_id];

                println!(
                    "Detected Move (Cross Library) (Inode): {:#?} -> {:#?}",
                    entry.path(),
                    tree.node(parent_idx).data().path
                );
                (
                    existing_id,
                    SyncOp::Move {
                        old_parent_id: filenode_track.0.parent_id,
                    },
                )
            } else {
                (Uuid::new_v4(), SyncOp::Insert)
            };

            let child_idx = tree.node_mut(parent_idx).push_child(FsNode {
                path: entry.path().into(),
                file_type,
                identity,
                size,
                mtime,
                db_id: node_id,
                parent_id: Some(parent_id),
                op,
            });

            if is_dir {
                if stack.len() >= depth {
                    stack.push(child_idx);
                } else {
                    stack[depth] = child_idx;
                }
            }
        }

        Ok(LibraryState { fs_tree: tree })
    }
}

pub struct LibraryState {
    pub fs_tree: DynTree<FsNode>,
}

impl LibraryState {
    pub async fn sync_with_db(
        &self,
        connection: &mut sqlx::SqliteConnection,
    ) -> anyhow::Result<()> {
        for node in self
            .fs_tree
            .root()
            .walk_with(&mut Traversal.dfs().over_nodes())
        {
            let data = node.data();
            match data.op {
                SyncOp::Synced => {}
                SyncOp::UpdateMeta => {
                    println!("update meta: {:#?}, {:#?}", data, data.mtime);

                    sqlx::query!(
                        r#"
                        UPDATE filenodes
                        SET
                            mtime = ?,
                            size = ?
                        WHERE id = ?;
                        "#,
                        data.mtime,
                        data.size as i64,
                        data.db_id,
                    )
                    .execute(&mut *connection)
                    .await?;
                }
                SyncOp::Insert => {
                    println!(
                        "insert {:#?}, {:#?}, {:#?}",
                        data.db_id, data.path, data.parent_id
                    );

                    data.insert_into_db(&mut *connection).await?;
                }
                SyncOp::Move { old_parent_id } => {
                    println!("moved: {:#?}, {:#?}", data, old_parent_id);

                    sqlx::query!(
                        r#"
                        UPDATE filenodes
                        SET parent_id = ?
                        WHERE id = ?;
                        "#,
                        data.parent_id,
                        data.db_id,
                    )
                    .execute(&mut *connection)
                    .await?;
                }
                SyncOp::NewRoot { library_id } => {
                    data.insert_into_db(&mut *connection).await?;
                    sqlx::query!(
                        r#"
                        UPDATE libraries
                        SET node = ?
                        WHERE id = ?;
                        "#,
                        data.db_id,
                        library_id
                    )
                    .execute(&mut *connection)
                    .await?;
                }
            }
        }

        Ok(())
    }
}

pub enum FetchLibraryRes {
    Snapshot(FxIndexMap<Uuid, Arc<Track>>),
}

pub enum MainIoMsg {
    FetchLibrary {
        reply: flume::Sender<FetchLibraryRes>,
    },
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

pub async fn io_thread(main_io_rx: flume::Receiver<MainIoMsg>) -> anyhow::Result<()> {
    let database_url =
        env::var("DATABASE_URL").expect("DATABASE_URL is not set in the environment");

    let mut state = PersistenceState {
        db: sqlx::Pool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::from_str(&database_url)?.create_if_missing(true),
        )
        .await?,
    };

    MIGRATOR.run(&state.db).await?;

    loop {
        let msg = main_io_rx.recv_async().await?;

        match msg {
            MainIoMsg::FetchLibrary { reply } => {
                state.handle_fetch_library(reply).await?;
            }
        }
    }

    // Ok(())
}
