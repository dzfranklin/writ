use std::{any::Any, fmt};

use lru::LruCache;
use tracing::warn;

use crate::{Object, Oid};

use super::UntypedOid;

pub(super) struct Cache(LruCache<UntypedOid, Box<dyn Any>>);

impl Cache {
    const CAPACITY: usize = 5000;

    pub(super) fn new() -> Self {
        Self(LruCache::new(Self::CAPACITY))
    }

    pub(super) fn insert<O: Object + 'static>(&mut self, oid: Oid<O>, object: O) {
        self.0.put(oid.clone().into_untyped(), Box::new(object));
    }

    pub(super) fn get<'c, O>(&'c mut self, oid: &Oid<O>) -> Option<&'c O>
    where
        O: Object + 'static,
    {
        let object = self.0.get(oid.as_untyped())?.downcast_ref::<O>();
        if object.is_none() {
            warn!("Object stored in cache under different type than requested");
        } else {
        }
        object
    }
}

impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cache")
            .field("capacity", &self.0.cap())
            .field("len", &self.0.len())
            .finish_non_exhaustive()
    }
}
