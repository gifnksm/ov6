struct buf;
struct context;
struct file;
struct inode;
struct pipe;
struct proc;
struct spinlock;
struct sleeplock;
struct stat;
struct superblock;

// kalloc.c
void *kalloc(void);

// printf.c
void panic(char *) __attribute__((noreturn));

// proc.c
int cpuid(void);
struct proc *myproc();
void sleep(void *, struct spinlock *);
void wakeup(void *);

// spinlock.c
void acquire(struct spinlock *);
void            initlock(struct spinlock*, char*);
void release(struct spinlock *);

// string.c
int             memcmp(const void*, const void*, uint);
void*           memmove(void*, const void*, uint);
void*           memset(void*, int, uint);
char*           safestrcpy(char*, const char*, int);
int             strlen(const char*);
int             strncmp(const char*, const char*, uint);
char*           strncpy(char*, const char*, int);

// virtio_disk.c
void            virtio_disk_init(void);
void            virtio_disk_rw(struct buf *, int);
void            virtio_disk_intr(void);
