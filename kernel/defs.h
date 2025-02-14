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

// exec.c
int             exec(char*, char**);

// file.c
struct file*    filealloc(void);
void            fileclose(struct file*);
struct file*    filedup(struct file*);
void            fileinit(void);
int             fileread(struct file*, uint64, int n);
int             filestat(struct file*, uint64 addr);
int             filewrite(struct file*, uint64, int n);

// fs.c
int             dirlink(struct inode*, char*, uint);
struct inode*   dirlookup(struct inode*, char*, uint*);
struct inode *ialloc(uint, short);
void            ilock(struct inode*);
void            iput(struct inode*);
void            iunlock(struct inode*);
void            iunlockput(struct inode*);
void            iupdate(struct inode*);
int             namecmp(const char*, const char*);
struct inode*   namei(char*);
struct inode*   nameiparent(char*, char*);
int readi(struct inode *, int, uint64, uint, uint);
int             writei(struct inode*, int, uint64, uint, uint);
void            itrunc(struct inode*);

// kalloc.c
void*           kalloc(void);
void            kfree(void *);

// log.c
void            begin_op(void);
void            end_op(void);

// pipe.c
int pipealloc(struct file **, struct file **);

// printf.c
int            printf(char*, ...) __attribute__ ((format (printf, 1, 2)));
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

// syscall.c
void            argint(int, int*);
int             argstr(int, char*, int);
void            argaddr(int, uint64 *);
int             fetchstr(uint64, char*, int);
int fetchaddr(uint64, uint64 *);

// vm.c
int copyout(pagetable_t, uint64, char *, uint64);

// virtio_disk.c
void            virtio_disk_init(void);
void            virtio_disk_rw(struct buf *, int);
void            virtio_disk_intr(void);

// number of elements in fixed-size array
#define NELEM(x) (sizeof(x)/sizeof((x)[0]))
