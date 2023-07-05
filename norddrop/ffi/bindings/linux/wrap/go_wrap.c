/* ----------------------------------------------------------------------------
 * This file was automatically generated by SWIG (http://www.swig.org).
 * Version 4.0.2
 *
 * This file is not intended to be easily readable and contains a number of
 * coding conventions designed to improve portability and efficiency. Do not make
 * changes to this file unless you know what you are doing--modify the SWIG
 * interface file instead.
 * ----------------------------------------------------------------------------- */

/* source: ffi/bindings/norddrop.i */

#define SWIGMODULE norddropgo
/* -----------------------------------------------------------------------------
 *  This section contains generic SWIG labels for method/variable
 *  declarations/attributes, and other compiler dependent labels.
 * ----------------------------------------------------------------------------- */

/* template workaround for compilers that cannot correctly implement the C++ standard */
#ifndef SWIGTEMPLATEDISAMBIGUATOR
# if defined(__SUNPRO_CC) && (__SUNPRO_CC <= 0x560)
#  define SWIGTEMPLATEDISAMBIGUATOR template
# elif defined(__HP_aCC)
/* Needed even with `aCC -AA' when `aCC -V' reports HP ANSI C++ B3910B A.03.55 */
/* If we find a maximum version that requires this, the test would be __HP_aCC <= 35500 for A.03.55 */
#  define SWIGTEMPLATEDISAMBIGUATOR template
# else
#  define SWIGTEMPLATEDISAMBIGUATOR
# endif
#endif

/* inline attribute */
#ifndef SWIGINLINE
# if defined(__cplusplus) || (defined(__GNUC__) && !defined(__STRICT_ANSI__))
#   define SWIGINLINE inline
# else
#   define SWIGINLINE
# endif
#endif

/* attribute recognised by some compilers to avoid 'unused' warnings */
#ifndef SWIGUNUSED
# if defined(__GNUC__)
#   if !(defined(__cplusplus)) || (__GNUC__ > 3 || (__GNUC__ == 3 && __GNUC_MINOR__ >= 4))
#     define SWIGUNUSED __attribute__ ((__unused__))
#   else
#     define SWIGUNUSED
#   endif
# elif defined(__ICC)
#   define SWIGUNUSED __attribute__ ((__unused__))
# else
#   define SWIGUNUSED
# endif
#endif

#ifndef SWIG_MSC_UNSUPPRESS_4505
# if defined(_MSC_VER)
#   pragma warning(disable : 4505) /* unreferenced local function has been removed */
# endif
#endif

#ifndef SWIGUNUSEDPARM
# ifdef __cplusplus
#   define SWIGUNUSEDPARM(p)
# else
#   define SWIGUNUSEDPARM(p) p SWIGUNUSED
# endif
#endif

/* internal SWIG method */
#ifndef SWIGINTERN
# define SWIGINTERN static SWIGUNUSED
#endif

/* internal inline SWIG method */
#ifndef SWIGINTERNINLINE
# define SWIGINTERNINLINE SWIGINTERN SWIGINLINE
#endif

/* exporting methods */
#if defined(__GNUC__)
#  if (__GNUC__ >= 4) || (__GNUC__ == 3 && __GNUC_MINOR__ >= 4)
#    ifndef GCC_HASCLASSVISIBILITY
#      define GCC_HASCLASSVISIBILITY
#    endif
#  endif
#endif

#ifndef SWIGEXPORT
# if defined(_WIN32) || defined(__WIN32__) || defined(__CYGWIN__)
#   if defined(STATIC_LINKED)
#     define SWIGEXPORT
#   else
#     define SWIGEXPORT __declspec(dllexport)
#   endif
# else
#   if defined(__GNUC__) && defined(GCC_HASCLASSVISIBILITY)
#     define SWIGEXPORT __attribute__ ((visibility("default")))
#   else
#     define SWIGEXPORT
#   endif
# endif
#endif

/* calling conventions for Windows */
#ifndef SWIGSTDCALL
# if defined(_WIN32) || defined(__WIN32__) || defined(__CYGWIN__)
#   define SWIGSTDCALL __stdcall
# else
#   define SWIGSTDCALL
# endif
#endif

/* Deal with Microsoft's attempt at deprecating C standard runtime functions */
#if !defined(SWIG_NO_CRT_SECURE_NO_DEPRECATE) && defined(_MSC_VER) && !defined(_CRT_SECURE_NO_DEPRECATE)
# define _CRT_SECURE_NO_DEPRECATE
#endif

/* Deal with Microsoft's attempt at deprecating methods in the standard C++ library */
#if !defined(SWIG_NO_SCL_SECURE_NO_DEPRECATE) && defined(_MSC_VER) && !defined(_SCL_SECURE_NO_DEPRECATE)
# define _SCL_SECURE_NO_DEPRECATE
#endif

/* Deal with Apple's deprecated 'AssertMacros.h' from Carbon-framework */
#if defined(__APPLE__) && !defined(__ASSERT_MACROS_DEFINE_VERSIONS_WITHOUT_UNDERSCORES)
# define __ASSERT_MACROS_DEFINE_VERSIONS_WITHOUT_UNDERSCORES 0
#endif

/* Intel's compiler complains if a variable which was never initialised is
 * cast to void, which is a common idiom which we use to indicate that we
 * are aware a variable isn't used.  So we just silence that warning.
 * See: https://github.com/swig/swig/issues/192 for more discussion.
 */
#ifdef __INTEL_COMPILER
# pragma warning disable 592
#endif


#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>



typedef int intgo;
typedef unsigned int uintgo;


# if !defined(__clang__) && (defined(__i386__) || defined(__x86_64__))
#   define SWIGSTRUCTPACKED __attribute__((__packed__, __gcc_struct__))
# else
#   define SWIGSTRUCTPACKED __attribute__((__packed__))
# endif



typedef struct { char *p; intgo n; } _gostring_;
typedef struct { void* array; intgo len; intgo cap; } _goslice_;




#define swiggo_size_assert_eq(x, y, name) typedef char name[(x-y)*(x-y)*-2+1];
#define swiggo_size_assert(t, n) swiggo_size_assert_eq(sizeof(t), n, swiggo_sizeof_##t##_is_not_##n)

swiggo_size_assert(char, 1)
swiggo_size_assert(short, 2)
swiggo_size_assert(int, 4)
typedef long long swiggo_long_long;
swiggo_size_assert(swiggo_long_long, 8)
swiggo_size_assert(float, 4)
swiggo_size_assert(double, 8)

#ifdef __cplusplus
extern "C" {
#endif
extern void crosscall2(void (*fn)(void *, int), void *, int);
extern char* _cgo_topofstack(void) __attribute__ ((weak));
extern void _cgo_allocate(void *, int);
extern void _cgo_panic(void *, int);
#ifdef __cplusplus
}
#endif

static char *_swig_topofstack() {
  if (_cgo_topofstack) {
    return _cgo_topofstack();
  } else {
    return 0;
  }
}

static void _swig_gopanic(const char *p) {
  struct {
    const char *p;
  } SWIGSTRUCTPACKED a;
  a.p = p;
  crosscall2(_cgo_panic, &a, (int) sizeof a);
}




#define SWIG_contract_assert(expr, msg) \
  if (!(expr)) { _swig_gopanic(msg); } else


static _gostring_ Swig_AllocateString(const char *p, size_t l) {
  _gostring_ ret;
  ret.p = (char*)malloc(l);
  memcpy(ret.p, p, l);
  ret.n = l;
  return ret;
}


static void Swig_free(void* p) {
  free(p);
}

static void* Swig_malloc(int c) {
  return malloc(c);
}


#include "../../norddrop.h"

SWIGINTERN struct norddrop *new_norddrop(norddrop_event_cb events,enum norddrop_log_level level,norddrop_logger_cb logger,norddrop_pubkey_cb pubkey_cb,char const *privkey){

        norddrop *t = NULL;
        if (NORDDROP_RES_OK != norddrop_new(&t, events, level, logger, pubkey_cb, privkey)) {
            return NULL;
        }
        return t;
    }
SWIGINTERN void delete_norddrop(struct norddrop *self){
        norddrop_destroy(self);
    }
#ifdef __cplusplus
extern "C" {
#endif

void _wrap_Swig_free_norddropgo_75c76e4825b5533c(void *_swig_go_0) {
  void *arg1 = (void *) 0 ;
  
  arg1 = *(void **)&_swig_go_0; 
  
  Swig_free(arg1);
  
}


void *_wrap_Swig_malloc_norddropgo_75c76e4825b5533c(intgo _swig_go_0) {
  int arg1 ;
  void *result = 0 ;
  void *_swig_go_result;
  
  arg1 = (int)_swig_go_0; 
  
  result = (void *)Swig_malloc(arg1);
  *(void **)&_swig_go_result = (void *)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPLOGCRITICAL_norddropgo_75c76e4825b5533c() {
  enum norddrop_log_level result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_LOG_CRITICAL;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPLOGERROR_norddropgo_75c76e4825b5533c() {
  enum norddrop_log_level result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_LOG_ERROR;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPLOGWARNING_norddropgo_75c76e4825b5533c() {
  enum norddrop_log_level result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_LOG_WARNING;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPLOGINFO_norddropgo_75c76e4825b5533c() {
  enum norddrop_log_level result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_LOG_INFO;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPLOGDEBUG_norddropgo_75c76e4825b5533c() {
  enum norddrop_log_level result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_LOG_DEBUG;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPLOGTRACE_norddropgo_75c76e4825b5533c() {
  enum norddrop_log_level result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_LOG_TRACE;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESOK_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_OK;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESERROR_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_ERROR;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESINVALIDSTRING_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_INVALID_STRING;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESBADINPUT_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_BAD_INPUT;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESJSONPARSE_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_JSON_PARSE;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESTRANSFERCREATE_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_TRANSFER_CREATE;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESNOTSTARTED_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_NOT_STARTED;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESADDRINUSE_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_ADDR_IN_USE;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESINSTANCESTART_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_INSTANCE_START;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESINSTANCESTOP_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_INSTANCE_STOP;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESINVALIDPRIVKEY_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_INVALID_PRIVKEY;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_NORDDROPRESDBERROR_norddropgo_75c76e4825b5533c() {
  enum norddrop_result result;
  intgo _swig_go_result;
  
  
  result = NORDDROP_RES_DB_ERROR;
  
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


void _wrap_NorddropEventCb_Ctx_set_norddropgo_75c76e4825b5533c(struct norddrop_event_cb *_swig_go_0, void *_swig_go_1) {
  struct norddrop_event_cb *arg1 = (struct norddrop_event_cb *) 0 ;
  void *arg2 = (void *) 0 ;
  
  arg1 = *(struct norddrop_event_cb **)&_swig_go_0; 
  arg2 = *(void **)&_swig_go_1; 
  
  if (arg1) (arg1)->ctx = arg2;
  
}


void *_wrap_NorddropEventCb_Ctx_get_norddropgo_75c76e4825b5533c(struct norddrop_event_cb *_swig_go_0) {
  struct norddrop_event_cb *arg1 = (struct norddrop_event_cb *) 0 ;
  void *result = 0 ;
  void *_swig_go_result;
  
  arg1 = *(struct norddrop_event_cb **)&_swig_go_0; 
  
  result = (void *) ((arg1)->ctx);
  *(void **)&_swig_go_result = (void *)result; 
  return _swig_go_result;
}


void _wrap_NorddropEventCb_Cb_set_norddropgo_75c76e4825b5533c(struct norddrop_event_cb *_swig_go_0, void* _swig_go_1) {
  struct norddrop_event_cb *arg1 = (struct norddrop_event_cb *) 0 ;
  norddrop_event_fn arg2 = (norddrop_event_fn) 0 ;
  
  arg1 = *(struct norddrop_event_cb **)&_swig_go_0; 
  arg2 = *(norddrop_event_fn *)&_swig_go_1; 
  
  if (arg1) (arg1)->cb = arg2;
  
}


void* _wrap_NorddropEventCb_Cb_get_norddropgo_75c76e4825b5533c(struct norddrop_event_cb *_swig_go_0) {
  struct norddrop_event_cb *arg1 = (struct norddrop_event_cb *) 0 ;
  norddrop_event_fn result;
  void* _swig_go_result;
  
  arg1 = *(struct norddrop_event_cb **)&_swig_go_0; 
  
  result = (norddrop_event_fn) ((arg1)->cb);
  *(norddrop_event_fn *)&_swig_go_result = (norddrop_event_fn)result; 
  return _swig_go_result;
}


struct norddrop_event_cb *_wrap_new_NorddropEventCb_norddropgo_75c76e4825b5533c() {
  struct norddrop_event_cb *result = 0 ;
  struct norddrop_event_cb *_swig_go_result;
  
  
  result = (struct norddrop_event_cb *)calloc(1, sizeof(struct norddrop_event_cb));
  *(struct norddrop_event_cb **)&_swig_go_result = (struct norddrop_event_cb *)result; 
  return _swig_go_result;
}


void _wrap_delete_NorddropEventCb_norddropgo_75c76e4825b5533c(struct norddrop_event_cb *_swig_go_0) {
  struct norddrop_event_cb *arg1 = (struct norddrop_event_cb *) 0 ;
  
  arg1 = *(struct norddrop_event_cb **)&_swig_go_0; 
  
  free((char *) arg1);
  
}


void _wrap_NorddropLoggerCb_Ctx_set_norddropgo_75c76e4825b5533c(struct norddrop_logger_cb *_swig_go_0, void *_swig_go_1) {
  struct norddrop_logger_cb *arg1 = (struct norddrop_logger_cb *) 0 ;
  void *arg2 = (void *) 0 ;
  
  arg1 = *(struct norddrop_logger_cb **)&_swig_go_0; 
  arg2 = *(void **)&_swig_go_1; 
  
  if (arg1) (arg1)->ctx = arg2;
  
}


void *_wrap_NorddropLoggerCb_Ctx_get_norddropgo_75c76e4825b5533c(struct norddrop_logger_cb *_swig_go_0) {
  struct norddrop_logger_cb *arg1 = (struct norddrop_logger_cb *) 0 ;
  void *result = 0 ;
  void *_swig_go_result;
  
  arg1 = *(struct norddrop_logger_cb **)&_swig_go_0; 
  
  result = (void *) ((arg1)->ctx);
  *(void **)&_swig_go_result = (void *)result; 
  return _swig_go_result;
}


void _wrap_NorddropLoggerCb_Cb_set_norddropgo_75c76e4825b5533c(struct norddrop_logger_cb *_swig_go_0, void* _swig_go_1) {
  struct norddrop_logger_cb *arg1 = (struct norddrop_logger_cb *) 0 ;
  norddrop_logger_fn arg2 = (norddrop_logger_fn) 0 ;
  
  arg1 = *(struct norddrop_logger_cb **)&_swig_go_0; 
  arg2 = *(norddrop_logger_fn *)&_swig_go_1; 
  
  if (arg1) (arg1)->cb = arg2;
  
}


void* _wrap_NorddropLoggerCb_Cb_get_norddropgo_75c76e4825b5533c(struct norddrop_logger_cb *_swig_go_0) {
  struct norddrop_logger_cb *arg1 = (struct norddrop_logger_cb *) 0 ;
  norddrop_logger_fn result;
  void* _swig_go_result;
  
  arg1 = *(struct norddrop_logger_cb **)&_swig_go_0; 
  
  result = (norddrop_logger_fn) ((arg1)->cb);
  *(norddrop_logger_fn *)&_swig_go_result = (norddrop_logger_fn)result; 
  return _swig_go_result;
}


struct norddrop_logger_cb *_wrap_new_NorddropLoggerCb_norddropgo_75c76e4825b5533c() {
  struct norddrop_logger_cb *result = 0 ;
  struct norddrop_logger_cb *_swig_go_result;
  
  
  result = (struct norddrop_logger_cb *)calloc(1, sizeof(struct norddrop_logger_cb));
  *(struct norddrop_logger_cb **)&_swig_go_result = (struct norddrop_logger_cb *)result; 
  return _swig_go_result;
}


void _wrap_delete_NorddropLoggerCb_norddropgo_75c76e4825b5533c(struct norddrop_logger_cb *_swig_go_0) {
  struct norddrop_logger_cb *arg1 = (struct norddrop_logger_cb *) 0 ;
  
  arg1 = *(struct norddrop_logger_cb **)&_swig_go_0; 
  
  free((char *) arg1);
  
}


void _wrap_NorddropPubkeyCb_Ctx_set_norddropgo_75c76e4825b5533c(struct norddrop_pubkey_cb *_swig_go_0, void *_swig_go_1) {
  struct norddrop_pubkey_cb *arg1 = (struct norddrop_pubkey_cb *) 0 ;
  void *arg2 = (void *) 0 ;
  
  arg1 = *(struct norddrop_pubkey_cb **)&_swig_go_0; 
  arg2 = *(void **)&_swig_go_1; 
  
  if (arg1) (arg1)->ctx = arg2;
  
}


void *_wrap_NorddropPubkeyCb_Ctx_get_norddropgo_75c76e4825b5533c(struct norddrop_pubkey_cb *_swig_go_0) {
  struct norddrop_pubkey_cb *arg1 = (struct norddrop_pubkey_cb *) 0 ;
  void *result = 0 ;
  void *_swig_go_result;
  
  arg1 = *(struct norddrop_pubkey_cb **)&_swig_go_0; 
  
  result = (void *) ((arg1)->ctx);
  *(void **)&_swig_go_result = (void *)result; 
  return _swig_go_result;
}


void _wrap_NorddropPubkeyCb_Cb_set_norddropgo_75c76e4825b5533c(struct norddrop_pubkey_cb *_swig_go_0, void* _swig_go_1) {
  struct norddrop_pubkey_cb *arg1 = (struct norddrop_pubkey_cb *) 0 ;
  norddrop_pubkey_fn arg2 = (norddrop_pubkey_fn) 0 ;
  
  arg1 = *(struct norddrop_pubkey_cb **)&_swig_go_0; 
  arg2 = *(norddrop_pubkey_fn *)&_swig_go_1; 
  
  if (arg1) (arg1)->cb = arg2;
  
}


void* _wrap_NorddropPubkeyCb_Cb_get_norddropgo_75c76e4825b5533c(struct norddrop_pubkey_cb *_swig_go_0) {
  struct norddrop_pubkey_cb *arg1 = (struct norddrop_pubkey_cb *) 0 ;
  norddrop_pubkey_fn result;
  void* _swig_go_result;
  
  arg1 = *(struct norddrop_pubkey_cb **)&_swig_go_0; 
  
  result = (norddrop_pubkey_fn) ((arg1)->cb);
  *(norddrop_pubkey_fn *)&_swig_go_result = (norddrop_pubkey_fn)result; 
  return _swig_go_result;
}


struct norddrop_pubkey_cb *_wrap_new_NorddropPubkeyCb_norddropgo_75c76e4825b5533c() {
  struct norddrop_pubkey_cb *result = 0 ;
  struct norddrop_pubkey_cb *_swig_go_result;
  
  
  result = (struct norddrop_pubkey_cb *)calloc(1, sizeof(struct norddrop_pubkey_cb));
  *(struct norddrop_pubkey_cb **)&_swig_go_result = (struct norddrop_pubkey_cb *)result; 
  return _swig_go_result;
}


void _wrap_delete_NorddropPubkeyCb_norddropgo_75c76e4825b5533c(struct norddrop_pubkey_cb *_swig_go_0) {
  struct norddrop_pubkey_cb *arg1 = (struct norddrop_pubkey_cb *) 0 ;
  
  arg1 = *(struct norddrop_pubkey_cb **)&_swig_go_0; 
  
  free((char *) arg1);
  
}


struct norddrop *_wrap_new_Norddrop_norddropgo_75c76e4825b5533c(norddrop_event_cb _swig_go_0, intgo _swig_go_1, norddrop_logger_cb _swig_go_2, norddrop_pubkey_cb _swig_go_3, _gostring_ _swig_go_4) {
  norddrop_event_cb arg1 ;
  enum norddrop_log_level arg2 ;
  norddrop_logger_cb arg3 ;
  norddrop_pubkey_cb arg4 ;
  char *arg5 = (char *) 0 ;
  struct norddrop *result = 0 ;
  struct norddrop *_swig_go_result;
  
  {
    arg1 = _swig_go_0;
  }
  arg2 = (enum norddrop_log_level)_swig_go_1; 
  {
    arg3 = _swig_go_2;
  }
  {
    arg4 = _swig_go_3;
  }
  
  arg5 = (char *)malloc(_swig_go_4.n + 1);
  memcpy(arg5, _swig_go_4.p, _swig_go_4.n);
  arg5[_swig_go_4.n] = '\0';
  
  
  result = (struct norddrop *)new_norddrop(arg1,arg2,arg3,arg4,(char const *)arg5);
  *(struct norddrop **)&_swig_go_result = (struct norddrop *)result; 
  free(arg5); 
  return _swig_go_result;
}


void _wrap_delete_Norddrop_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  delete_norddrop(arg1);
  
}


intgo _wrap_Norddrop_Start_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, _gostring_ _swig_go_1, _gostring_ _swig_go_2) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  arg2 = (char *)malloc(_swig_go_1.n + 1);
  memcpy(arg2, _swig_go_1.p, _swig_go_1.n);
  arg2[_swig_go_1.n] = '\0';
  
  
  arg3 = (char *)malloc(_swig_go_2.n + 1);
  memcpy(arg3, _swig_go_2.p, _swig_go_2.n);
  arg3[_swig_go_2.n] = '\0';
  
  
  result = (enum norddrop_result)norddrop_start(arg1,(char const *)arg2,(char const *)arg3);
  _swig_go_result = (intgo)result; 
  free(arg2); 
  free(arg3); 
  return _swig_go_result;
}


intgo _wrap_Norddrop_Stop_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  result = (enum norddrop_result)norddrop_stop(arg1);
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_Norddrop_CancelTransfer_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, _gostring_ _swig_go_1) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  arg2 = (char *)malloc(_swig_go_1.n + 1);
  memcpy(arg2, _swig_go_1.p, _swig_go_1.n);
  arg2[_swig_go_1.n] = '\0';
  
  
  result = (enum norddrop_result)norddrop_cancel_transfer(arg1,(char const *)arg2);
  _swig_go_result = (intgo)result; 
  free(arg2); 
  return _swig_go_result;
}


intgo _wrap_Norddrop_CancelFile_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, _gostring_ _swig_go_1, _gostring_ _swig_go_2) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  arg2 = (char *)malloc(_swig_go_1.n + 1);
  memcpy(arg2, _swig_go_1.p, _swig_go_1.n);
  arg2[_swig_go_1.n] = '\0';
  
  
  arg3 = (char *)malloc(_swig_go_2.n + 1);
  memcpy(arg3, _swig_go_2.p, _swig_go_2.n);
  arg3[_swig_go_2.n] = '\0';
  
  
  result = (enum norddrop_result)norddrop_cancel_file(arg1,(char const *)arg2,(char const *)arg3);
  _swig_go_result = (intgo)result; 
  free(arg2); 
  free(arg3); 
  return _swig_go_result;
}


intgo _wrap_Norddrop_RejectFile_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, _gostring_ _swig_go_1, _gostring_ _swig_go_2) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  arg2 = (char *)malloc(_swig_go_1.n + 1);
  memcpy(arg2, _swig_go_1.p, _swig_go_1.n);
  arg2[_swig_go_1.n] = '\0';
  
  
  arg3 = (char *)malloc(_swig_go_2.n + 1);
  memcpy(arg3, _swig_go_2.p, _swig_go_2.n);
  arg3[_swig_go_2.n] = '\0';
  
  
  result = (enum norddrop_result)norddrop_reject_file(arg1,(char const *)arg2,(char const *)arg3);
  _swig_go_result = (intgo)result; 
  free(arg2); 
  free(arg3); 
  return _swig_go_result;
}


intgo _wrap_Norddrop_Download_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, _gostring_ _swig_go_1, _gostring_ _swig_go_2, _gostring_ _swig_go_3) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  char *arg4 = (char *) 0 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  arg2 = (char *)malloc(_swig_go_1.n + 1);
  memcpy(arg2, _swig_go_1.p, _swig_go_1.n);
  arg2[_swig_go_1.n] = '\0';
  
  
  arg3 = (char *)malloc(_swig_go_2.n + 1);
  memcpy(arg3, _swig_go_2.p, _swig_go_2.n);
  arg3[_swig_go_2.n] = '\0';
  
  
  arg4 = (char *)malloc(_swig_go_3.n + 1);
  memcpy(arg4, _swig_go_3.p, _swig_go_3.n);
  arg4[_swig_go_3.n] = '\0';
  
  
  result = (enum norddrop_result)norddrop_download(arg1,(char const *)arg2,(char const *)arg3,(char const *)arg4);
  _swig_go_result = (intgo)result; 
  free(arg2); 
  free(arg3); 
  free(arg4); 
  return _swig_go_result;
}


_gostring_ _wrap_Norddrop_NewTransfer_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, _gostring_ _swig_go_1, _gostring_ _swig_go_2) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  char *result = 0 ;
  _gostring_ _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  arg2 = (char *)malloc(_swig_go_1.n + 1);
  memcpy(arg2, _swig_go_1.p, _swig_go_1.n);
  arg2[_swig_go_1.n] = '\0';
  
  
  arg3 = (char *)malloc(_swig_go_2.n + 1);
  memcpy(arg3, _swig_go_2.p, _swig_go_2.n);
  arg3[_swig_go_2.n] = '\0';
  
  
  result = (char *)norddrop_new_transfer(arg1,(char const *)arg2,(char const *)arg3);
  _swig_go_result = Swig_AllocateString((char*)result, result ? strlen((char*)result) : 0); 
  free(arg2); 
  free(arg3); 
  free(result);
  return _swig_go_result;
}


intgo _wrap_Norddrop_PurgeTransfers_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, _gostring_ _swig_go_1) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  arg2 = (char *)malloc(_swig_go_1.n + 1);
  memcpy(arg2, _swig_go_1.p, _swig_go_1.n);
  arg2[_swig_go_1.n] = '\0';
  
  
  result = (enum norddrop_result)norddrop_purge_transfers(arg1,(char const *)arg2);
  _swig_go_result = (intgo)result; 
  free(arg2); 
  return _swig_go_result;
}


intgo _wrap_Norddrop_PurgeTransfersUntil_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, long long _swig_go_1) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  long long arg2 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  arg2 = (long long)_swig_go_1; 
  
  result = (enum norddrop_result)norddrop_purge_transfers_until(arg1,arg2);
  _swig_go_result = (intgo)result; 
  return _swig_go_result;
}


intgo _wrap_Norddrop_RemoveTransferFile_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, _gostring_ _swig_go_1, _gostring_ _swig_go_2) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  enum norddrop_result result;
  intgo _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  
  arg2 = (char *)malloc(_swig_go_1.n + 1);
  memcpy(arg2, _swig_go_1.p, _swig_go_1.n);
  arg2[_swig_go_1.n] = '\0';
  
  
  arg3 = (char *)malloc(_swig_go_2.n + 1);
  memcpy(arg3, _swig_go_2.p, _swig_go_2.n);
  arg3[_swig_go_2.n] = '\0';
  
  
  result = (enum norddrop_result)norddrop_remove_transfer_file(arg1,(char const *)arg2,(char const *)arg3);
  _swig_go_result = (intgo)result; 
  free(arg2); 
  free(arg3); 
  return _swig_go_result;
}


_gostring_ _wrap_Norddrop_GetTransfersSince_norddropgo_75c76e4825b5533c(struct norddrop *_swig_go_0, long long _swig_go_1) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  long long arg2 ;
  char *result = 0 ;
  _gostring_ _swig_go_result;
  
  arg1 = *(struct norddrop **)&_swig_go_0; 
  arg2 = (long long)_swig_go_1; 
  
  result = (char *)norddrop_get_transfers_since(arg1,arg2);
  _swig_go_result = Swig_AllocateString((char*)result, result ? strlen((char*)result) : 0); 
  free(result);
  return _swig_go_result;
}


_gostring_ _wrap_Norddrop_Version_norddropgo_75c76e4825b5533c() {
  char *result = 0 ;
  _gostring_ _swig_go_result;
  
  
  result = (char *)norddrop_version();
  _swig_go_result = Swig_AllocateString((char*)result, result ? strlen((char*)result) : 0); 
  return _swig_go_result;
}


#ifdef __cplusplus
}
#endif

