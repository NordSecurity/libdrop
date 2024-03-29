/* ----------------------------------------------------------------------------
 * This file was automatically generated by SWIG (http://www.swig.org).
 * Version 4.0.2
 *
 * Do not make changes to this file unless you know what you are doing--modify
 * the SWIG interface file instead.
 * ----------------------------------------------------------------------------- */

package com.nordsec.norddrop;

public final class NorddropLogLevel {
  public final static NorddropLogLevel NORDDROP_LOG_CRITICAL = new NorddropLogLevel("NORDDROP_LOG_CRITICAL", libnorddropJNI.NORDDROP_LOG_CRITICAL_get());
  public final static NorddropLogLevel NORDDROP_LOG_ERROR = new NorddropLogLevel("NORDDROP_LOG_ERROR", libnorddropJNI.NORDDROP_LOG_ERROR_get());
  public final static NorddropLogLevel NORDDROP_LOG_WARNING = new NorddropLogLevel("NORDDROP_LOG_WARNING", libnorddropJNI.NORDDROP_LOG_WARNING_get());
  public final static NorddropLogLevel NORDDROP_LOG_INFO = new NorddropLogLevel("NORDDROP_LOG_INFO", libnorddropJNI.NORDDROP_LOG_INFO_get());
  public final static NorddropLogLevel NORDDROP_LOG_DEBUG = new NorddropLogLevel("NORDDROP_LOG_DEBUG", libnorddropJNI.NORDDROP_LOG_DEBUG_get());
  public final static NorddropLogLevel NORDDROP_LOG_TRACE = new NorddropLogLevel("NORDDROP_LOG_TRACE", libnorddropJNI.NORDDROP_LOG_TRACE_get());

  public final int swigValue() {
    return swigValue;
  }

  public String toString() {
    return swigName;
  }

  public static NorddropLogLevel swigToEnum(int swigValue) {
    if (swigValue < swigValues.length && swigValue >= 0 && swigValues[swigValue].swigValue == swigValue)
      return swigValues[swigValue];
    for (int i = 0; i < swigValues.length; i++)
      if (swigValues[i].swigValue == swigValue)
        return swigValues[i];
    throw new IllegalArgumentException("No enum " + NorddropLogLevel.class + " with value " + swigValue);
  }

  private NorddropLogLevel(String swigName) {
    this.swigName = swigName;
    this.swigValue = swigNext++;
  }

  private NorddropLogLevel(String swigName, int swigValue) {
    this.swigName = swigName;
    this.swigValue = swigValue;
    swigNext = swigValue+1;
  }

  private NorddropLogLevel(String swigName, NorddropLogLevel swigEnum) {
    this.swigName = swigName;
    this.swigValue = swigEnum.swigValue;
    swigNext = this.swigValue+1;
  }

  private static NorddropLogLevel[] swigValues = { NORDDROP_LOG_CRITICAL, NORDDROP_LOG_ERROR, NORDDROP_LOG_WARNING, NORDDROP_LOG_INFO, NORDDROP_LOG_DEBUG, NORDDROP_LOG_TRACE };
  private static int swigNext = 0;
  private final int swigValue;
  private final String swigName;
}

