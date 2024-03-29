/* ----------------------------------------------------------------------------
 * This file was automatically generated by SWIG (http://www.swig.org).
 * Version 4.0.2
 *
 * Do not make changes to this file unless you know what you are doing--modify
 * the SWIG interface file instead.
 * ----------------------------------------------------------------------------- */

package com.nordsec.norddrop;

public final class NorddropResult {
  public final static NorddropResult NORDDROP_RES_OK = new NorddropResult("NORDDROP_RES_OK", libnorddropJNI.NORDDROP_RES_OK_get());
  public final static NorddropResult NORDDROP_RES_ERROR = new NorddropResult("NORDDROP_RES_ERROR", libnorddropJNI.NORDDROP_RES_ERROR_get());
  public final static NorddropResult NORDDROP_RES_INVALID_STRING = new NorddropResult("NORDDROP_RES_INVALID_STRING", libnorddropJNI.NORDDROP_RES_INVALID_STRING_get());
  public final static NorddropResult NORDDROP_RES_BAD_INPUT = new NorddropResult("NORDDROP_RES_BAD_INPUT", libnorddropJNI.NORDDROP_RES_BAD_INPUT_get());
  public final static NorddropResult NORDDROP_RES_JSON_PARSE = new NorddropResult("NORDDROP_RES_JSON_PARSE", libnorddropJNI.NORDDROP_RES_JSON_PARSE_get());
  public final static NorddropResult NORDDROP_RES_TRANSFER_CREATE = new NorddropResult("NORDDROP_RES_TRANSFER_CREATE", libnorddropJNI.NORDDROP_RES_TRANSFER_CREATE_get());
  public final static NorddropResult NORDDROP_RES_NOT_STARTED = new NorddropResult("NORDDROP_RES_NOT_STARTED", libnorddropJNI.NORDDROP_RES_NOT_STARTED_get());
  public final static NorddropResult NORDDROP_RES_ADDR_IN_USE = new NorddropResult("NORDDROP_RES_ADDR_IN_USE", libnorddropJNI.NORDDROP_RES_ADDR_IN_USE_get());
  public final static NorddropResult NORDDROP_RES_INSTANCE_START = new NorddropResult("NORDDROP_RES_INSTANCE_START", libnorddropJNI.NORDDROP_RES_INSTANCE_START_get());
  public final static NorddropResult NORDDROP_RES_INSTANCE_STOP = new NorddropResult("NORDDROP_RES_INSTANCE_STOP", libnorddropJNI.NORDDROP_RES_INSTANCE_STOP_get());
  public final static NorddropResult NORDDROP_RES_INVALID_PRIVKEY = new NorddropResult("NORDDROP_RES_INVALID_PRIVKEY", libnorddropJNI.NORDDROP_RES_INVALID_PRIVKEY_get());
  public final static NorddropResult NORDDROP_RES_DB_ERROR = new NorddropResult("NORDDROP_RES_DB_ERROR", libnorddropJNI.NORDDROP_RES_DB_ERROR_get());

  public final int swigValue() {
    return swigValue;
  }

  public String toString() {
    return swigName;
  }

  public static NorddropResult swigToEnum(int swigValue) {
    if (swigValue < swigValues.length && swigValue >= 0 && swigValues[swigValue].swigValue == swigValue)
      return swigValues[swigValue];
    for (int i = 0; i < swigValues.length; i++)
      if (swigValues[i].swigValue == swigValue)
        return swigValues[i];
    throw new IllegalArgumentException("No enum " + NorddropResult.class + " with value " + swigValue);
  }

  private NorddropResult(String swigName) {
    this.swigName = swigName;
    this.swigValue = swigNext++;
  }

  private NorddropResult(String swigName, int swigValue) {
    this.swigName = swigName;
    this.swigValue = swigValue;
    swigNext = swigValue+1;
  }

  private NorddropResult(String swigName, NorddropResult swigEnum) {
    this.swigName = swigName;
    this.swigValue = swigEnum.swigValue;
    swigNext = this.swigValue+1;
  }

  private static NorddropResult[] swigValues = { NORDDROP_RES_OK, NORDDROP_RES_ERROR, NORDDROP_RES_INVALID_STRING, NORDDROP_RES_BAD_INPUT, NORDDROP_RES_JSON_PARSE, NORDDROP_RES_TRANSFER_CREATE, NORDDROP_RES_NOT_STARTED, NORDDROP_RES_ADDR_IN_USE, NORDDROP_RES_INSTANCE_START, NORDDROP_RES_INSTANCE_STOP, NORDDROP_RES_INVALID_PRIVKEY, NORDDROP_RES_DB_ERROR };
  private static int swigNext = 0;
  private final int swigValue;
  private final String swigName;
}

