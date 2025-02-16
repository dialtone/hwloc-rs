use bitmap::IntHwlocBitmap;
use libc::{c_char, c_int, c_uint, c_ulonglong};
use num::{FromPrimitive, ToPrimitive};
use std::cmp::{Ordering, PartialOrd};
use support::TopologySupport;
use topology_object::TopologyObject;
use {pid_t, pthread_t};

pub enum HwlocTopology {}

/// Represents the type of a topology object.
///
/// Note that (partial) ordering for object types is implemented as a call
/// into the `hwloc` library which defines ordering as follows:
///
/// - A == B if `ObjectType::A` and `ObjectType::B` are the same.
/// - A < B if `ObjectType::A` includes objects of type `ObjectType::B`.
/// - A > B if objects of `ObjectType::A` are included in type `ObjectType::B`.
///
/// It can also help to think of it as comparing the relative depths of each type, so
/// a `ObjectType::System` will be smaller than a `ObjectType::PU` since the system
/// contains processing units.
#[repr(u32)]
#[derive(Debug, Clone)]
pub enum ObjectType {
    /// This type is always used for the root object of a topology,
    /// and never used anywhere else.
    /// Hence its parent is always \c NULL
    /// A set of processors and memory with cache coherency.
    Machine,
    /// Physical package, what goes into a socket. In the physical meaning,
    /// i.e. that you can add or remove physically.
    Package,
    /// A computation unit (may be shared by several logical processors).
    Core,
    /// Processing Unit, or (Logical) Processor.
    ///
    /// An execution unit (may share a core with some other logical
    /// processors, e.g. in the case of an SMT core). Objects of this kind
    /// are always reported and can thus be used as fallback when others are
    /// not.
    PU,
    /// Level 1 Data (or Unified) Cache.
    L1Cache,
    /// Level 2 Data (or Unified) Cache.
    L2Cache,
    /// Level 3 Data (or Unified) Cache.
    L3Cache,
    /// Level 4 Data (or Unified) Cache.
    L4Cache,
    /// Level 5 Data (or Unified) Cache.
    L5Cache,
    /// Level 1 instruction Cache (filtered out by default).
    L1ICache,
    /// Level 2 instruction Cache (filtered out by default).
    L2ICache,
    /// Level 3 instruction Cache (filtered out by default).
    L3ICache,
    /// Group objects.
    ///
    /// Objects which do not fit in the above but are detected by hwloc and
    /// are useful to take into account for affinity. For instance, some
    /// operating systems expose their arbitrary processors aggregation this
    /// way. And hwloc may insert such objects to group NUMA nodes according
    /// to their distances.
    ///
    /// These objects are ignored when they do not bring any structure.
    Group,
    /// A set of processors around memory which the processors can directly
    /// access.
    NUMANode,
    /// Bridge (filtered out by default).
    /// Any bridge that connects the host or an I/O bus, to another I/O bus.
    /// They are not added to the topology unless I/O discovery
    /// is enabled with hwloc_topology_set_flags().
    /// I/O objects are not listed in the main children list,
    /// but rather in the dedicated io children list.
    /// I/O objects have NULL CPU and node sets.
    Bridge,
    /// PCI device.
    ///
    /// These objects have neither CPU sets nor node sets.
    /// They are not added to the topology unless I/O discovery
    /// is enabled through the custom flags.
    PCIDevice,
    /// Operating system device.
    ///
    /// These objects have neither CPU sets nor node sets. They are not
    /// added to the topology unless I/O discovery is enabled
    /// through the custom flags.
    OSDevice,
    /// Miscellaneous objects (filtered out by default).
    /// Objects without particular meaning, that can e.g. be
    /// added by the application for its own use, or by hwloc
    /// for miscellaneous objects such as MemoryModule (DIMMs).
    /// These objects are not listed in the main children list,
    /// but rather in the dedicated misc children list.
    /// Misc objects may only have Misc objects as children,
    /// and those are in the dedicated misc children list as well.
    /// Misc objects have NULL CPU and node sets.
    Misc,
    /// Memory-side cache (filtered out by default).
    /// A cache in front of a specific NUMA node.
    /// This object always has at least one NUMA node as a memory child.
    /// Memory objects are not listed in the main children list,
    /// but rather in the dedicated Memory children list.
    /// Memory-side cache have a special depth ::HWLOC_TYPE_DEPTH_MEMCACHE
    /// instead of a normal depth just like other objects in the
    /// main tree.
    Memcache,
    /// Die within a physical package.
    /// A subpart of the physical package, that contains multiple cores.
    Die,
    /// An internal sentinel value.
    TypeMax,
}

impl PartialOrd for ObjectType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let compared = unsafe { hwloc_compare_types(self.clone(), other.clone()) };
        match compared {
            c if c < 0 => Some(Ordering::Less),
            c if c == 0 => Some(Ordering::Equal),
            c if c > 0 => Some(Ordering::Greater),
            _ => None,
        }
    }
}

impl PartialEq for ObjectType {
    fn eq(&self, other: &Self) -> bool {
        match self.partial_cmp(other) {
            Some(Ordering::Equal) => true,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TypeDepthError {
    /// No object of given type exists in the topology.
    TypeDepthUnknown = -1,
    /// Objects of given type exist at different depth in the topology.
    TypeDepthMultiple = -2,
    /// Virtual depth for NUMA nodes.
    TypeDepthNumaNode = -3,
    /// Virtual depth for bridge object level.
    TypeDepthBridge = -4,
    /// Virtual depth for PCI device object level.
    TypeDepthPCIDevice = -5,
    /// Virtual depth for software device object level.
    TypeDepthOSDevice = -6,
    /// Virtual depth for Misc object.
    TypeDepthMisc = -7,
    /// Virtual depth for MemCache object.
    TypeDepthMemcache = -8,
    /// stuff
    Unkown = -99,
}

#[derive(Debug, PartialEq)]
pub enum CacheType {
    Unified,
    Data,
    Instruction,
}

#[derive(Debug, PartialEq)]
pub enum BridgeType {
    Host,
    Pci,
}

#[derive(Debug, PartialEq)]
pub enum OsDevType {
    Block,
    Gpu,
    Network,
    OpenFabrics,
    Dma,
    Coproc,
}

const TOPOLOGY_FLAG_INCLUDE_DISALLOWED: i64 = 1;
const TOPOLOGY_FLAG_IS_THIS_SYSTEM: i64 = 2;
const TOPOLOGY_FLAG_THISSYSTEM_ALLOWED_RESOURCES: i64 = 4;

#[derive(Debug, PartialEq)]
pub enum TopologyFlag {
    IncludeDisallowed = TOPOLOGY_FLAG_INCLUDE_DISALLOWED as isize,
    IsThisSystem = TOPOLOGY_FLAG_IS_THIS_SYSTEM as isize,
    ThisSystemAllowedResources = TOPOLOGY_FLAG_THISSYSTEM_ALLOWED_RESOURCES as isize,
}

impl ToPrimitive for TopologyFlag {
    fn to_i64(&self) -> Option<i64> {
        match *self {
            TopologyFlag::IsThisSystem => Some(TopologyFlag::IsThisSystem as i64),
            TopologyFlag::IncludeDisallowed => Some(TopologyFlag::IncludeDisallowed as i64),
            TopologyFlag::ThisSystemAllowedResources => {
                Some(TopologyFlag::ThisSystemAllowedResources as i64)
            }
        }
    }

    fn to_u64(&self) -> Option<u64> {
        self.to_i64().and_then(|x| x.to_u64())
    }
}

impl FromPrimitive for TopologyFlag {
    fn from_i64(n: i64) -> Option<Self> {
        match n {
            TOPOLOGY_FLAG_IS_THIS_SYSTEM => Some(TopologyFlag::IsThisSystem),
            TOPOLOGY_FLAG_INCLUDE_DISALLOWED => Some(TopologyFlag::IncludeDisallowed),
            TOPOLOGY_FLAG_THISSYSTEM_ALLOWED_RESOURCES => {
                Some(TopologyFlag::ThisSystemAllowedResources)
            }
            _ => None,
        }
    }

    fn from_u64(n: u64) -> Option<Self> {
        FromPrimitive::from_i64(n as i64)
    }
}

#[cfg(target_os = "windows")]
#[link(name = "libhwloc")]
extern "C" {

    // === Topology Creation and Destruction ===

    pub fn hwloc_topology_init(topology: *mut *mut HwlocTopology) -> c_int;
    pub fn hwloc_topology_load(topology: *mut HwlocTopology) -> c_int;
    pub fn hwloc_topology_destroy(topology: *mut HwlocTopology);

    // === Topology Detection Configuration and Query ===

    pub fn hwloc_topology_set_flags(topology: *mut HwlocTopology, flags: c_ulonglong) -> c_int;
    pub fn hwloc_topology_get_flags(topology: *mut HwlocTopology) -> c_ulonglong;
    pub fn hwloc_topology_get_support(topology: *mut HwlocTopology) -> *const TopologySupport;

    // === Object levels, depths and types ===

    pub fn hwloc_topology_get_depth(topology: *mut HwlocTopology) -> c_uint;
    pub fn hwloc_get_type_depth(topology: *mut HwlocTopology, object_type: ObjectType) -> c_int;
    pub fn hwloc_get_depth_type(topology: *mut HwlocTopology, depth: c_uint) -> ObjectType;
    pub fn hwloc_get_nbobjs_by_depth(topology: *mut HwlocTopology, depth: c_uint) -> c_uint;

    pub fn hwloc_get_obj_by_depth(
        topology: *mut HwlocTopology,
        depth: c_uint,
        idx: c_uint,
    ) -> *mut TopologyObject;

    // === CPU Binding ===
    pub fn hwloc_set_cpubind(
        topology: *mut HwlocTopology,
        set: *const IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_cpubind(
        topology: *mut HwlocTopology,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_last_cpu_location(
        topology: *mut HwlocTopology,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_proc_last_cpu_location(
        topology: *mut HwlocTopology,
        pid: pid_t,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_set_proc_cpubind(
        topology: *mut HwlocTopology,
        pid: pid_t,
        set: *const IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_proc_cpubind(
        topology: *mut HwlocTopology,
        pid: pid_t,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_set_thread_cpubind(
        topology: *mut HwlocTopology,
        thread: pthread_t,
        set: *const IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_thread_cpubind(
        topology: *mut HwlocTopology,
        pid: pthread_t,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;

    // === Memory Binding ===

    // === Bitmap Methods ===
    pub fn hwloc_bitmap_alloc() -> *mut IntHwlocBitmap;
    pub fn hwloc_bitmap_alloc_full() -> *mut IntHwlocBitmap;
    pub fn hwloc_bitmap_free(bitmap: *mut IntHwlocBitmap);
    pub fn hwloc_bitmap_list_asprintf(
        strp: *mut *mut c_char,
        bitmap: *const IntHwlocBitmap,
    ) -> c_int;
    pub fn hwloc_bitmap_set(bitmap: *mut IntHwlocBitmap, id: c_uint);
    pub fn hwloc_bitmap_set_range(bitmap: *mut IntHwlocBitmap, begin: c_uint, end: c_int);
    pub fn hwloc_bitmap_clr(bitmap: *mut IntHwlocBitmap, id: c_uint);
    pub fn hwloc_bitmap_clr_range(bitmap: *mut IntHwlocBitmap, begin: c_uint, end: c_int);
    pub fn hwloc_bitmap_weight(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_zero(bitmap: *mut IntHwlocBitmap);
    pub fn hwloc_bitmap_iszero(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_isset(bitmap: *const IntHwlocBitmap, id: c_uint) -> c_int;
    pub fn hwloc_bitmap_singlify(bitmap: *mut IntHwlocBitmap);
    pub fn hwloc_bitmap_not(result: *mut IntHwlocBitmap, bitmap: *const IntHwlocBitmap);
    pub fn hwloc_bitmap_first(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_last(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_dup(src: *const IntHwlocBitmap) -> *mut IntHwlocBitmap;
    pub fn hwloc_bitmap_compare(left: *const IntHwlocBitmap, right: *const IntHwlocBitmap)
        -> c_int;
    pub fn hwloc_bitmap_isequal(left: *const IntHwlocBitmap, right: *const IntHwlocBitmap)
        -> c_int;
    pub fn hwloc_bitmap_isfull(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_next(bitmap: *const IntHwlocBitmap, prev: c_int) -> c_int;

    pub fn hwloc_obj_type_snprintf(
        into: *mut c_char,
        size: c_int,
        object: *const TopologyObject,
        verbose: bool,
    ) -> c_int;
    pub fn hwloc_obj_attr_snprintf(
        into: *mut c_char,
        size: c_int,
        object: *const TopologyObject,
        separator: *const c_char,
        verbose: bool,
    ) -> c_int;

    pub fn hwloc_compare_types(type1: ObjectType, type2: ObjectType) -> c_int;
}

#[cfg(not(target_os = "windows"))]
#[link(name = "hwloc")]
extern "C" {

    // === Topology Creation and Destruction ===

    pub fn hwloc_topology_init(topology: *mut *mut HwlocTopology) -> c_int;
    pub fn hwloc_topology_load(topology: *mut HwlocTopology) -> c_int;
    pub fn hwloc_topology_destroy(topology: *mut HwlocTopology);

    // === Topology Detection Configuration and Query ===

    pub fn hwloc_topology_set_flags(topology: *mut HwlocTopology, flags: c_ulonglong) -> c_int;
    pub fn hwloc_topology_get_flags(topology: *mut HwlocTopology) -> c_ulonglong;
    pub fn hwloc_topology_get_support(topology: *mut HwlocTopology) -> *const TopologySupport;

    // === Object levels, depths and types ===

    pub fn hwloc_topology_get_depth(topology: *mut HwlocTopology) -> c_uint;
    pub fn hwloc_get_type_depth(topology: *mut HwlocTopology, object_type: ObjectType) -> c_int;
    pub fn hwloc_get_depth_type(topology: *mut HwlocTopology, depth: c_uint) -> ObjectType;
    pub fn hwloc_get_nbobjs_by_depth(topology: *mut HwlocTopology, depth: c_uint) -> c_uint;

    pub fn hwloc_get_obj_by_depth(
        topology: *mut HwlocTopology,
        depth: c_uint,
        idx: c_uint,
    ) -> *mut TopologyObject;

    // === CPU Binding ===
    pub fn hwloc_set_cpubind(
        topology: *mut HwlocTopology,
        set: *const IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_cpubind(
        topology: *mut HwlocTopology,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_last_cpu_location(
        topology: *mut HwlocTopology,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_proc_last_cpu_location(
        topology: *mut HwlocTopology,
        pid: pid_t,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_set_proc_cpubind(
        topology: *mut HwlocTopology,
        pid: pid_t,
        set: *const IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_proc_cpubind(
        topology: *mut HwlocTopology,
        pid: pid_t,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_set_thread_cpubind(
        topology: *mut HwlocTopology,
        thread: pthread_t,
        set: *const IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;
    pub fn hwloc_get_thread_cpubind(
        topology: *mut HwlocTopology,
        pid: pthread_t,
        set: *mut IntHwlocBitmap,
        flags: c_int,
    ) -> c_int;

    // === Memory Binding ===

    // === Bitmap Methods ===
    pub fn hwloc_bitmap_alloc() -> *mut IntHwlocBitmap;
    pub fn hwloc_bitmap_alloc_full() -> *mut IntHwlocBitmap;
    pub fn hwloc_bitmap_free(bitmap: *mut IntHwlocBitmap);
    pub fn hwloc_bitmap_list_asprintf(
        strp: *mut *mut c_char,
        bitmap: *const IntHwlocBitmap,
    ) -> c_int;
    pub fn hwloc_bitmap_set(bitmap: *mut IntHwlocBitmap, id: c_uint);
    pub fn hwloc_bitmap_set_range(bitmap: *mut IntHwlocBitmap, begin: c_uint, end: c_int);
    pub fn hwloc_bitmap_clr(bitmap: *mut IntHwlocBitmap, id: c_uint);
    pub fn hwloc_bitmap_clr_range(bitmap: *mut IntHwlocBitmap, begin: c_uint, end: c_int);
    pub fn hwloc_bitmap_weight(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_zero(bitmap: *mut IntHwlocBitmap);
    pub fn hwloc_bitmap_iszero(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_isset(bitmap: *const IntHwlocBitmap, id: c_uint) -> c_int;
    pub fn hwloc_bitmap_singlify(bitmap: *mut IntHwlocBitmap);
    pub fn hwloc_bitmap_not(result: *mut IntHwlocBitmap, bitmap: *const IntHwlocBitmap);
    pub fn hwloc_bitmap_first(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_last(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_dup(src: *const IntHwlocBitmap) -> *mut IntHwlocBitmap;
    pub fn hwloc_bitmap_compare(left: *const IntHwlocBitmap, right: *const IntHwlocBitmap)
        -> c_int;
    pub fn hwloc_bitmap_isequal(left: *const IntHwlocBitmap, right: *const IntHwlocBitmap)
        -> c_int;
    pub fn hwloc_bitmap_isfull(bitmap: *const IntHwlocBitmap) -> c_int;
    pub fn hwloc_bitmap_next(bitmap: *const IntHwlocBitmap, prev: c_int) -> c_int;

    pub fn hwloc_obj_type_snprintf(
        into: *mut c_char,
        size: c_int,
        object: *const TopologyObject,
        verbose: bool,
    ) -> c_int;
    pub fn hwloc_obj_attr_snprintf(
        into: *mut c_char,
        size: c_int,
        object: *const TopologyObject,
        separator: *const c_char,
        verbose: bool,
    ) -> c_int;

    pub fn hwloc_compare_types(type1: ObjectType, type2: ObjectType) -> c_int;
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn should_convert_flag_to_primitive() {
        assert_eq!(1, TopologyFlag::IncludeDisallowed as u64);
        assert_eq!(2, TopologyFlag::IsThisSystem as u64);
    }

    #[test]
    fn should_compare_object_types() {
        assert!(ObjectType::Machine == ObjectType::Machine);
        assert!(ObjectType::PU == ObjectType::PU);

        assert!(ObjectType::Machine < ObjectType::PU);
        assert!(ObjectType::PU > ObjectType::L1Cache);
    }
}
