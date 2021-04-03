use std::{collections::HashMap, path::{Path, PathBuf}};

use steamworks::PublishedFileId;

use crate::{GMAMetadata, gma::{GMAError, GMAFile}, transactions::Transaction};

use super::{THREAD_POOL, PromiseCache};

pub struct GMA {
	cache: PromiseCache<HashMap<PathBuf, GMAFile>, PathBuf, Result<GMAFile, GMAError>>,
}

unsafe impl Sync for GMA {}
unsafe impl Send for GMA {}

impl GMA {
	pub fn init() -> GMA {
		Self {
			cache: PromiseCache::new(HashMap::new()),
		}
	}

	pub fn get<P: AsRef<Path>>(&'static self, path: P, id: Option<PublishedFileId>) -> Result<GMAFile, GMAError> {
		main_thread_forbidden!();

		let path = path.as_ref();

		let mut gma = GMAFile::open(path)?;
		if let Some(id) = id {
			gma.set_ws_id(id);
		}
		gma.metadata()?;
		gma.entries()?;

		{
			let path = path.to_path_buf();
			let gma = gma.clone();
			self.cache.write(move |mut cache| {
				cache.insert(path, gma);
			});
		}

		Ok(gma)
	}

	pub fn get_async<P: AsRef<Path>, F>(&'static self, path: P, id: Option<PublishedFileId>, f: F)
	where
		F: FnOnce(&Result<GMAFile, GMAError>) + 'static + Send,
	{
		let path = path.as_ref();
		match self.cache.read().get(path) {
			Some(gma) => {
				let mut gma = gma.clone();
				gma.id = id;
				f(&Ok(gma));
			}
			None => {
				let path = path.to_path_buf();
				if self.cache.task(path.clone(), f) {
					THREAD_POOL.spawn(move || {
						let v = self.get(&path, id);
						crate::GMA.cache.execute(&path, v);
					});
				}
			}
		}
	}

	pub fn create(&'static self, src_path: PathBuf, dest_path: PathBuf, data: GMAMetadata) -> Result<Transaction, GMAError> {
		crate::gma::GMAFile::write(&src_path, &dest_path, &data)
	}

	pub fn create_async<F>(&'static self, src_path: PathBuf, dest_path: PathBuf, data: GMAMetadata, f: F)
	where
		F: FnOnce(&Result<Transaction, GMAError>) + 'static + Send,
	{
		THREAD_POOL.spawn(move || f(&self.create(src_path, dest_path, data)));
	}
}
