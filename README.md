# Connected Data Lake Prototype Implementation

## Installation

```bash
# Install Rook-Ceph Cluster
./ceph-install.sh

# Install MinIO Object Store
./minio-install.sh

# Install Data Pond Cluster & Pipelines
./pond-install.sh
```

## Check Status

### Ceph Cluster

```bash
./ceph-status.sh
```

<details>
<summary>Expected Outputs</summary>

<pre>
autodata-hot-storage-1:
  cluster:
    id:     [HIDDEN]
    health: HEALTH_WARN
            1 pool(s) have no replicas configured
 
  services:
    mon: 1 daemons, quorum a (age 3w)
    mgr: a(active, since 3w)
    osd: 47 osds: 47 up (since 3w), 47 in (since 3w)
 
  data:
    pools:   2 pools, 33 pgs
    objects: 7.06k objects, 17 GiB
    usage:   43 GiB used, 247 TiB / 247 TiB avail
    pgs:     33 active+clean
 
  io:
    client:   13 KiB/s wr, 0 op/s rd, 2 op/s wr
 
autodata-hot-storage-2:
  cluster:
    id:     [HIDDEN]
    health: HEALTH_WARN
            1 pool(s) have no replicas configured
 
  services:
    mon: 1 daemons, quorum a (age 3w)
    mgr: a(active, since 3w)
    osd: 20 osds: 20 up (since 3w), 20 in (since 3w)
 
  data:
    pools:   2 pools, 33 pgs
    objects: 7.07k objects, 17 GiB
    usage:   34 GiB used, 140 TiB / 140 TiB avail
    pgs:     33 active+clean
 
  io:
    client:   9.3 KiB/s wr, 0 op/s rd, 1 op/s wr
 
autodata-warm-storage-1:
  cluster:
    id:     [HIDDEN]
    health: HEALTH_WARN
            2 pool(s) have no replicas configured
 
  services:
    mon: 1 daemons, quorum a (age 3w)
    mgr: a(active, since 3w)
    osd: 2 osds: 2 up (since 3w), 2 in (since 3w)
 
  data:
    pools:   2 pools, 33 pgs
    objects: 7.11k objects, 17 GiB
    usage:   6.0 GiB used, 590 TiB / 590 TiB avail
    pgs:     33 active+clean
 
  io:
    client:   31 KiB/s wr, 0 op/s rd, 4 op/s wr
 
mobilex:
  cluster:
    id:     [HIDDEN]
    health: HEALTH_OK
 
  services:
    mon: 3 daemons, quorum a,b,c (age 5w)
    mgr: b(active, since 7h), standbys: a
    mds: 1/1 daemons up, 1 hot standby
    osd: 24 osds: 24 up (since 5w), 24 in (since 4M)
 
  data:
    volumes: 1/1 healthy
    pools:   19 pools, 409 pgs
    objects: 3.21M objects, 2.0 TiB
    usage:   5.8 TiB used, 134 TiB / 140 TiB avail
    pgs:     409 active+clean
 
  io:
    client:   45 KiB/s rd, 287 KiB/s wr, 11 op/s rd, 23 op/s wr
 
netai-cloud:
  cluster:
    id:     [HIDDEN]
    health: HEALTH_WARN
            2 pool(s) have no replicas configured
 
  services:
    mon: 1 daemons, quorum a (age 3w)
    mgr: a(active, since 3w)
    osd: 31 osds: 31 up (since 3w), 31 in (since 3w)
 
  data:
    pools:   2 pools, 33 pgs
    objects: 544 objects, 1.1 GiB
    usage:   1.1 GiB used, 311 TiB / 311 TiB avail
    pgs:     33 active+clean
</pre>

</details>

### Data Pond

```bash
./pond-status.sh
```

<details>
<summary>Expected Outputs</summary>

<pre>
[ autodata-hot-storage-1 ]
NAMESPACE   NAME       STATE   CREATED-AT   UPDATED-AT
tenant-b    tenant-a   Ready   23d          23d
tenant-b    tenant-b   Ready   23d          23d
tenant-b    tenant-c   Ready   23d          23d
tenant-b    tenant-d   Ready   23d          23d
[ autodata-hot-storage-2 ]
NAMESPACE   NAME       STATE   CREATED-AT   UPDATED-AT
tenant-c    tenant-a   Ready   23d          23d
tenant-c    tenant-b   Ready   23d          23d
tenant-c    tenant-c   Ready   23d          23d
tenant-c    tenant-d   Ready   23d          23d
[ autodata-warm-storage-1 ]
NAMESPACE   NAME       STATE   CREATED-AT   UPDATED-AT
tenant-a    tenant-a   Ready   23d          23d
tenant-a    tenant-b   Ready   23d          23d
tenant-a    tenant-c   Ready   23d          23d
tenant-a    tenant-d   Ready   23d          23d
[ mobilex ]
NAMESPACE   NAME       STATE   CREATED-AT   UPDATED-AT
tenant-d    tenant-a   Ready   23d          23d
tenant-d    tenant-b   Ready   23d          23d
tenant-d    tenant-c   Ready   23d          23d
tenant-d    tenant-d   Ready   23d          23d
[ netai-cloud ]
No resources found
</pre>

</details>
